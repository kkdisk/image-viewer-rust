//! HEIF 家族（HEIC／AVIF）容器的有界探測：容器家族、主影像實際編碼與色彩資訊。
//!
//! 只讀取頂層 box 標頭與 `meta`，以 Seek 略過 `mdat`；`meta` 超過上限即視為無法判定。
//! 所有切片存取都經過邊界檢查，截斷或損毀的資料回傳錯誤，不會 panic。

use std::io::{Read, Seek, SeekFrom};

/// `meta` box 的讀取上限；iPhone 樣本約 4 KiB，留足餘裕。
pub const META_LIMIT: u64 = 512 * 1024;
/// 最多檢查的頂層 box 數，避免以大量空 box 拖延。
const MAX_TOP_BOXES: usize = 32;
/// 追蹤 `dimg` 衍生關係（grid、tmap 等）的深度上限。
const MAX_DERIVATION_DEPTH: usize = 4;

/// 主影像的實際編碼。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Hevc,
    Av1,
    /// 其他已知項目類型（例如 `jpeg`），保留原始四字元碼。
    Other([u8; 4]),
    /// 無法由項目或品牌判定。
    Unknown,
}

impl Codec {
    pub fn label(self) -> String {
        match self {
            Codec::Hevc => "HEVC".into(),
            Codec::Av1 => "AV1".into(),
            Codec::Other(code) => String::from_utf8_lossy(&code).trim().to_owned(),
            Codec::Unknown => "未知編碼".into(),
        }
    }
}

/// 主影像關聯的色彩資訊。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorInfo {
    None,
    Icc(Vec<u8>),
    Nclx {
        primaries: u16,
        transfer: u16,
        matrix: u16,
        full_range: bool,
    },
}

impl ColorInfo {
    /// `nclx` 的色域是否為 sRGB／BT.709（1）或未指定（2）。
    pub fn is_srgb_like(&self) -> bool {
        match self {
            ColorInfo::None => true,
            ColorInfo::Icc(_) => false,
            ColorInfo::Nclx { primaries, .. } => matches!(primaries, 1 | 2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeifInfo {
    pub codec: Codec,
    pub color: ColorInfo,
    /// 主品牌，例如 `heic`、`avif`、`mif1`。
    pub major_brand: [u8; 4],
}

const FAMILY_BRANDS: &[&[u8; 4]] = &[
    b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"miaf", b"avif",
    b"avis",
];
const HEVC_BRANDS: &[&[u8; 4]] = &[b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx"];
const AV1_BRANDS: &[&[u8; 4]] = &[b"avif", b"avis"];

/// 由檔頭判斷是否屬於 HEIF 家族容器（只看 `ftyp`，不判定實際編碼）。
pub fn is_heif_family(header: &[u8]) -> bool {
    ftyp_brands(header).is_some_and(|(major, compatible)| {
        FAMILY_BRANDS.contains(&&major) || compatible.iter().any(|b| FAMILY_BRANDS.contains(&b))
    })
}

fn ftyp_brands(header: &[u8]) -> Option<([u8; 4], Vec<[u8; 4]>)> {
    let size = u32::from_be_bytes(header.get(0..4)?.try_into().ok()?) as usize;
    if header.get(4..8)? != b"ftyp" || size < 16 {
        return None;
    }
    let body = header.get(8..size.min(header.len()))?;
    let major: [u8; 4] = body.get(0..4)?.try_into().ok()?;
    let compatible = body.get(8..)?.as_chunks::<4>().0.to_vec();
    Some((major, compatible))
}

/// 有界的大端讀取游標。
struct Cur<'a> {
    data: &'a [u8],
    pos: usize,
}

type Res<T> = Result<T, &'static str>;
const BROKEN: &str = "HEIF 容器資料損毀或截斷";

impl<'a> Cur<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(BROKEN)?;
        let slice = self.data.get(self.pos..end).ok_or(BROKEN)?;
        self.pos = end;
        Ok(slice)
    }
    fn uint(&mut self, n: usize) -> Res<u64> {
        Ok(self
            .take(n)?
            .iter()
            .fold(0, |acc, b| (acc << 8) | u64::from(*b)))
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        Ok(self.uint(2)? as u16)
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(self.uint(4)? as u32)
    }
    fn fourcc(&mut self) -> Res<[u8; 4]> {
        self.take(4)?.try_into().map_err(|_| BROKEN)
    }
    /// FullBox 標頭：版本與旗標。
    fn full_header(&mut self) -> Res<(u8, u32)> {
        let version = self.u8()?;
        let flags = self.uint(3)? as u32;
        Ok((version, flags))
    }
    fn rest(&mut self) -> &'a [u8] {
        let rest = self.data.get(self.pos..).unwrap_or(&[]);
        self.pos = self.data.len();
        rest
    }
}

/// 逐一走訪子 box，回傳（類型, 內容）。
fn child_boxes(data: &[u8]) -> Res<Vec<([u8; 4], &[u8])>> {
    let mut out = Vec::new();
    let mut cur = Cur::new(data);
    while cur.pos < data.len() {
        let start = cur.pos;
        let size = cur.u32()? as u64;
        let kind = cur.fourcc()?;
        let (header, size) = match size {
            0 => (8, (data.len() - start) as u64),
            1 => (16, cur.uint(8)?),
            n => (8, n),
        };
        let size = usize::try_from(size).map_err(|_| BROKEN)?;
        if size < header {
            return Err(BROKEN);
        }
        let end = start.checked_add(size).ok_or(BROKEN)?;
        let body = data.get(start + header..end).ok_or(BROKEN)?;
        out.push((kind, body));
        cur.pos = end;
    }
    Ok(out)
}

/// 探測檔案：讀 `ftyp` 與 `meta`，以 Seek 略過其他頂層 box。
pub fn probe<R: Read + Seek>(reader: &mut R) -> Res<HeifInfo> {
    reader.seek(SeekFrom::Start(0)).map_err(|_| BROKEN)?;
    let mut major = None;
    let mut compatible = Vec::new();
    for _ in 0..MAX_TOP_BOXES {
        let mut header = [0u8; 8];
        if reader.read_exact(&mut header).is_err() {
            break;
        }
        let size32 = u32::from_be_bytes(header[0..4].try_into().map_err(|_| BROKEN)?) as u64;
        let kind: [u8; 4] = header[4..8].try_into().map_err(|_| BROKEN)?;
        let (header_len, size) = match size32 {
            0 => (8, None),
            1 => {
                let mut large = [0u8; 8];
                reader.read_exact(&mut large).map_err(|_| BROKEN)?;
                (16, Some(u64::from_be_bytes(large)))
            }
            n => (8, Some(n)),
        };
        let body_len = match size {
            Some(size) => size.checked_sub(header_len).ok_or(BROKEN)?,
            None if &kind == b"mdat" => break,
            None => META_LIMIT + 1,
        };
        match &kind {
            b"ftyp" | b"meta" => {
                if body_len > META_LIMIT {
                    return Err("HEIF 中繼資料超過讀取上限");
                }
                let mut body = Vec::new();
                reader
                    .by_ref()
                    .take(body_len)
                    .read_to_end(&mut body)
                    .map_err(|_| BROKEN)?;
                if body.len() as u64 != body_len {
                    return Err(BROKEN);
                }
                if &kind == b"ftyp" {
                    let mut cur = Cur::new(&body);
                    major = Some(cur.fourcc()?);
                    cur.u32()?;
                    compatible = cur.rest().as_chunks::<4>().0.to_vec();
                } else {
                    let major = major.ok_or("HEIF 缺少 ftyp")?;
                    return parse_meta(&body, major, &compatible);
                }
            }
            _ => {
                let skip = i64::try_from(body_len).map_err(|_| BROKEN)?;
                reader.seek(SeekFrom::Current(skip)).map_err(|_| BROKEN)?;
            }
        }
    }
    Err("HEIF 缺少 meta")
}

/// 探測記憶體中的資料。
pub fn probe_bytes(bytes: &[u8]) -> Res<HeifInfo> {
    probe(&mut std::io::Cursor::new(bytes))
}

#[derive(Default)]
struct Meta {
    primary: Option<u32>,
    item_types: Vec<(u32, [u8; 4])>,
    /// `dimg` 參照：衍生影像 → 來源影像。
    derived_from: Vec<(u32, Vec<u32>)>,
    properties: Vec<([u8; 4], Vec<u8>)>,
    associations: Vec<(u32, Vec<u16>)>,
}

fn parse_meta(body: &[u8], major: [u8; 4], compatible: &[[u8; 4]]) -> Res<HeifInfo> {
    let mut cur = Cur::new(body);
    cur.full_header()?;
    let mut meta = Meta::default();
    for (kind, data) in child_boxes(cur.rest())? {
        match &kind {
            b"pitm" => {
                let mut c = Cur::new(data);
                let (version, _) = c.full_header()?;
                meta.primary = Some(if version == 0 {
                    c.u16()?.into()
                } else {
                    c.u32()?
                });
            }
            b"iinf" => parse_iinf(data, &mut meta)?,
            b"iref" => parse_iref(data, &mut meta)?,
            b"iprp" => parse_iprp(data, &mut meta)?,
            _ => {}
        }
    }
    let primary = meta.primary.ok_or("HEIF 缺少主影像")?;
    let codec = resolve_codec(&meta, primary, 0).unwrap_or_else(|| brand_codec(major, compatible));
    let color = resolve_color(&meta, primary, 0).unwrap_or(ColorInfo::None);
    Ok(HeifInfo {
        codec,
        color,
        major_brand: major,
    })
}

fn parse_iinf(data: &[u8], meta: &mut Meta) -> Res<()> {
    let mut c = Cur::new(data);
    let (version, _) = c.full_header()?;
    if version == 0 {
        c.u16()?;
    } else {
        c.u32()?;
    }
    for (kind, infe) in child_boxes(c.rest())? {
        if &kind != b"infe" {
            continue;
        }
        let mut e = Cur::new(infe);
        let (version, _) = e.full_header()?;
        if version < 2 {
            continue;
        }
        let id = if version == 2 {
            e.u16()?.into()
        } else {
            e.u32()?
        };
        e.u16()?;
        meta.item_types.push((id, e.fourcc()?));
    }
    Ok(())
}

fn parse_iref(data: &[u8], meta: &mut Meta) -> Res<()> {
    let mut c = Cur::new(data);
    let (version, _) = c.full_header()?;
    let id_len = if version == 0 { 2 } else { 4 };
    for (kind, reference) in child_boxes(c.rest())? {
        if &kind != b"dimg" {
            continue;
        }
        let mut r = Cur::new(reference);
        let from = r.uint(id_len)? as u32;
        let count = r.u16()?;
        let to = (0..count)
            .map(|_| r.uint(id_len).map(|v| v as u32))
            .collect::<Res<Vec<_>>>()?;
        meta.derived_from.push((from, to));
    }
    Ok(())
}

fn parse_iprp(data: &[u8], meta: &mut Meta) -> Res<()> {
    for (kind, inner) in child_boxes(data)? {
        match &kind {
            b"ipco" => {
                for (property, body) in child_boxes(inner)? {
                    meta.properties.push((property, body.to_vec()));
                }
            }
            b"ipma" => {
                let mut c = Cur::new(inner);
                let (version, flags) = c.full_header()?;
                for _ in 0..c.u32()? {
                    let id = if version < 1 {
                        c.u16()?.into()
                    } else {
                        c.u32()?
                    };
                    let count = c.u8()?;
                    let mut indexes = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        let index = if flags & 1 == 1 {
                            c.u16()? & 0x7fff
                        } else {
                            u16::from(c.u8()? & 0x7f)
                        };
                        indexes.push(index);
                    }
                    meta.associations.push((id, indexes));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn resolve_codec(meta: &Meta, item: u32, depth: usize) -> Option<Codec> {
    let kind = meta.item_types.iter().find(|(id, _)| *id == item)?.1;
    match &kind {
        b"hvc1" => Some(Codec::Hevc),
        b"av01" => Some(Codec::Av1),
        // 衍生影像（grid、tmap 等）依其 `dimg` 來源判定；沒有來源者即以自身類型回報。
        _ if depth < MAX_DERIVATION_DEPTH => Some(
            meta.derived_from
                .iter()
                .find(|(from, _)| *from == item)
                .and_then(|(_, sources)| {
                    sources
                        .iter()
                        .find_map(|source| resolve_codec(meta, *source, depth + 1))
                })
                .unwrap_or(Codec::Other(kind)),
        ),
        _ => Some(Codec::Other(kind)),
    }
}

fn resolve_color(meta: &Meta, item: u32, depth: usize) -> Option<ColorInfo> {
    let own = meta
        .associations
        .iter()
        .find(|(id, _)| *id == item)
        .into_iter()
        .flat_map(|(_, indexes)| indexes)
        .filter_map(|index| meta.properties.get((*index as usize).checked_sub(1)?))
        .find(|(kind, _)| kind == b"colr")
        .and_then(|(_, body)| parse_colr(body));
    own.or_else(|| {
        if depth >= MAX_DERIVATION_DEPTH {
            return None;
        }
        let sources = &meta.derived_from.iter().find(|(from, _)| *from == item)?.1;
        sources
            .iter()
            .find_map(|source| resolve_color(meta, *source, depth + 1))
    })
}

fn parse_colr(body: &[u8]) -> Option<ColorInfo> {
    let mut c = Cur::new(body);
    match &c.fourcc().ok()? {
        b"nclx" => Some(ColorInfo::Nclx {
            primaries: c.u16().ok()?,
            transfer: c.u16().ok()?,
            matrix: c.u16().ok()?,
            full_range: c.u8().ok()? & 0x80 != 0,
        }),
        b"prof" | b"rICC" => Some(ColorInfo::Icc(c.rest().to_vec())),
        _ => None,
    }
}

/// 項目資訊不足時，以品牌輔助判定。
fn brand_codec(major: [u8; 4], compatible: &[[u8; 4]]) -> Codec {
    let brands = std::iter::once(&major).chain(compatible);
    let mut hevc = false;
    let mut av1 = false;
    for brand in brands {
        hevc |= HEVC_BRANDS.contains(&brand);
        av1 |= AV1_BRANDS.contains(&brand);
    }
    match (hevc, av1) {
        (true, false) => Codec::Hevc,
        (false, true) => Codec::Av1,
        _ => Codec::Unknown,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }
    fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
        let mut data = vec![version];
        data.extend_from_slice(&flags.to_be_bytes()[1..]);
        data.extend_from_slice(body);
        boxed(kind, &data)
    }
    fn infe(id: u16, kind: &[u8; 4]) -> Vec<u8> {
        let mut body = id.to_be_bytes().to_vec();
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(kind);
        body.push(0);
        full(b"infe", 2, 0, &body)
    }

    /// 合成最小 HEIF：品牌、項目（id, 類型）、`dimg` 參照、主影像色彩，`mdat` 可選擇放在 `meta` 前。
    pub(crate) fn synth(
        major: &[u8; 4],
        compatible: &[&[u8; 4]],
        items: &[(u16, &[u8; 4])],
        dimg: Option<(u16, &[u16])>,
        primary: u16,
        colr: Option<Vec<u8>>,
        mdat_first: usize,
    ) -> Vec<u8> {
        let mut ftyp = major.to_vec();
        ftyp.extend_from_slice(&0u32.to_be_bytes());
        for brand in compatible {
            ftyp.extend_from_slice(*brand);
        }
        let mut meta = Vec::new();
        meta.extend(full(b"hdlr", 0, 0, &[0; 20]));
        meta.extend(full(b"pitm", 0, 0, &primary.to_be_bytes()));
        let mut iinf = (items.len() as u16).to_be_bytes().to_vec();
        for (id, kind) in items {
            iinf.extend(infe(*id, kind));
        }
        meta.extend(full(b"iinf", 0, 0, &iinf));
        if let Some((from, to)) = dimg {
            let mut reference = from.to_be_bytes().to_vec();
            reference.extend_from_slice(&(to.len() as u16).to_be_bytes());
            for id in to {
                reference.extend_from_slice(&id.to_be_bytes());
            }
            meta.extend(full(b"iref", 0, 0, &boxed(b"dimg", &reference)));
        }
        if let Some(colr) = colr {
            let ipco = boxed(b"ipco", &boxed(b"colr", &colr));
            let mut ipma = 1u32.to_be_bytes().to_vec();
            ipma.extend_from_slice(&primary.to_be_bytes());
            ipma.push(1);
            ipma.push(0x81);
            meta.extend(boxed(b"iprp", &[ipco, full(b"ipma", 0, 0, &ipma)].concat()));
        }
        let mut file = boxed(b"ftyp", &ftyp);
        if mdat_first > 0 {
            file.extend(boxed(b"mdat", &vec![0; mdat_first]));
        }
        file.extend(full(b"meta", 0, 0, &meta));
        file.extend(boxed(b"mdat", &[0; 16]));
        file
    }

    fn nclx(primaries: u16) -> Vec<u8> {
        let mut c = b"nclx".to_vec();
        for v in [primaries, 13, 6] {
            c.extend_from_slice(&v.to_be_bytes());
        }
        c.push(0x80);
        c
    }

    #[test]
    fn 品牌只判定容器家族_編碼以主影像項目為準() {
        let heic = synth(b"heic", &[b"mif1"], &[(1, b"hvc1")], None, 1, None, 0);
        assert!(is_heif_family(&heic));
        assert_eq!(probe_bytes(&heic).unwrap().codec, Codec::Hevc);
        // 通用品牌 mif1 也用於 AVIF：不得推定為 HEVC。
        let avif = synth(b"mif1", &[b"miaf"], &[(1, b"av01")], None, 1, None, 0);
        assert!(is_heif_family(&avif));
        assert_eq!(probe_bytes(&avif).unwrap().codec, Codec::Av1);
        let jpeg_item = synth(b"mif1", &[], &[(1, b"jpeg")], None, 1, None, 0);
        assert_eq!(
            probe_bytes(&jpeg_item).unwrap().codec,
            Codec::Other(*b"jpeg")
        );
        assert!(!is_heif_family(
            b"\x00\x00\x00\x18ftypisom\x00\x00\x00\x00isomiso2"
        ));
        assert!(!is_heif_family(b"\xff\xd8\xff\xe0"));
    }

    #[test]
    fn grid_依分塊類型判定並沿用衍生來源的色彩() {
        let file = synth(
            b"heic",
            &[b"mif1"],
            &[(1, b"hvc1"), (2, b"hvc1"), (9, b"grid")],
            Some((9, &[1, 2])),
            9,
            Some(nclx(12)),
            0,
        );
        let info = probe_bytes(&file).unwrap();
        assert_eq!(info.codec, Codec::Hevc);
        assert!(!info.color.is_srgb_like(), "P3（12）不是 sRGB");
        let avif_grid = synth(
            b"avif",
            &[],
            &[(1, b"av01"), (9, b"grid")],
            Some((9, &[1])),
            9,
            Some(nclx(1)),
            0,
        );
        let info = probe_bytes(&avif_grid).unwrap();
        assert_eq!(info.codec, Codec::Av1);
        assert!(info.color.is_srgb_like());
    }

    #[test]
    fn 擷取_icc_與品牌後備判定() {
        let mut prof = b"prof".to_vec();
        prof.extend_from_slice(b"ICC-BYTES");
        let file = synth(b"heic", &[], &[(1, b"hvc1")], None, 1, Some(prof), 0);
        assert_eq!(
            probe_bytes(&file).unwrap().color,
            ColorInfo::Icc(b"ICC-BYTES".to_vec())
        );
        // 主影像項目資訊缺失時，才以品牌輔助。
        let unknown = synth(b"heic", &[], &[], None, 7, None, 0);
        assert_eq!(probe_bytes(&unknown).unwrap().codec, Codec::Hevc);
        let generic = synth(b"mif1", &[b"miaf"], &[], None, 7, None, 0);
        assert_eq!(probe_bytes(&generic).unwrap().codec, Codec::Unknown);
    }

    #[test]
    fn mdat_在前以_seek_略過且只讀取標頭與_meta() {
        struct Counting<R>(R, u64);
        impl<R: Read> Read for Counting<R> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.read(buf)?;
                self.1 += n as u64;
                Ok(n)
            }
        }
        impl<R: Seek> Seek for Counting<R> {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                self.0.seek(pos)
            }
        }
        let file = synth(b"heic", &[], &[(1, b"hvc1")], None, 1, None, 4 << 20);
        let mut reader = Counting(std::io::Cursor::new(&file), 0);
        assert_eq!(probe(&mut reader).unwrap().codec, Codec::Hevc);
        assert!(reader.1 < 1024, "不得讀取 mdat 內容：實讀 {}", reader.1);
    }

    #[test]
    fn 截斷損毀與超過上限皆回傳錯誤而不_panic() {
        let file = synth(b"heic", &[], &[(1, b"hvc1")], None, 1, Some(nclx(1)), 0);
        for cut in 0..file.len() {
            let _ = probe_bytes(&file[..cut]);
        }
        let mut corrupted = file.clone();
        for i in (0..corrupted.len()).step_by(3) {
            corrupted[i] ^= 0xA5;
            let _ = probe_bytes(&corrupted);
        }
        // 宣告超大的 meta：不得配置或讀取。
        let mut huge = synth(b"heic", &[], &[], None, 1, None, 0);
        let meta_at = huge.windows(4).position(|w| w == b"meta").unwrap() - 4;
        huge[meta_at..meta_at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(probe_bytes(&huge), Err("HEIF 中繼資料超過讀取上限"));
        assert!(probe_bytes(b"").is_err());
    }
}
