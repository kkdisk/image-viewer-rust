//! 視口縮放、平移與雙圖同步狀態管理

/// 視口縮放與平移狀態
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportState {
    /// 縮放倍率 (1.0 代表 100%)
    pub zoom: f32,
    /// 畫布平移位移 (dx, dy)，單位為像素
    pub pan: (f32, f32),
    /// 是否啟用「自適應視窗大小」
    pub auto_fit: bool,
    /// 雙圖視口同步開關
    pub sync_enabled: bool,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: (0.0, 0.0),
            auto_fit: true,
            sync_enabled: false,
        }
    }
}

impl ViewportState {
    /// 建立新的預設視口
    pub fn new() -> Self {
        Self::default()
    }

    /// 重設平移與縮放至 100% 原始尺寸
    pub fn reset_actual_size(&mut self) {
        self.zoom = 1.0;
        self.pan = (0.0, 0.0);
        self.auto_fit = false;
    }

    /// 重設為最適視窗模式
    pub fn reset_fit_window(&mut self) {
        self.pan = (0.0, 0.0);
        self.auto_fit = true;
    }

    /// 拖曳平移
    pub fn pan_by(&mut self, delta_x: f32, delta_y: f32) {
        self.pan.0 += delta_x;
        self.pan.1 += delta_y;
        self.auto_fit = false;
    }

    /// 依游標位置進行縮放 (Zoom at cursor anchor)
    ///
    /// factor > 1.0 為放大，factor < 1.0 為縮小
    /// cursor_pos 為視窗畫布座標中的游標位置
    pub fn zoom_at(&mut self, factor: f32, cursor_x: f32, cursor_y: f32) {
        let old_zoom = self.zoom;
        let new_zoom = (self.zoom * factor).clamp(0.02, 50.0);

        if (new_zoom - old_zoom).abs() > 1e-6 {
            // 以游標為錨點調整平移偏移，使游標下方的圖片點保持不變
            let scale_change = new_zoom / old_zoom;
            self.pan.0 = cursor_x - (cursor_x - self.pan.0) * scale_change;
            self.pan.1 = cursor_y - (cursor_y - self.pan.1) * scale_change;
            self.zoom = new_zoom;
            self.auto_fit = false;
        }
    }

    /// 將視口狀態同步至另一個視口
    pub fn sync_to(&self, other: &mut ViewportState) {
        if self.sync_enabled {
            other.zoom = self.zoom;
            other.pan = self.pan;
            other.auto_fit = self.auto_fit;
        }
    }
}
