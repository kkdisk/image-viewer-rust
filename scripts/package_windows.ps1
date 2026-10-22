$ErrorActionPreference = 'Stop'
$workspacePath = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $workspacePath
try {
    $metadataJson = cargo metadata --no-deps --format-version 1 --locked --offline
    if ($LASTEXITCODE -ne 0) { throw '讀取套件版本失敗' }
    $metadata = $metadataJson | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object { $_.name -eq 'app-ui' }).version
    if (-not $version) { throw '找不到應用程式版本' }
    if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne "v$version") {
        throw "標籤與套件版本不一致：$($env:GITHUB_REF_NAME)，預期 v$version"
    }
    $executable = Join-Path $metadata.target_directory 'release/app-ui.exe'
    $size = (Get-Item -LiteralPath $executable).Length
    if ($size -ge 15MB) { throw "執行檔超過體積限制：$size 位元組" }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    $archive = "dist/rust-image-viewer-$version-windows-x64.zip"
    Compress-Archive -LiteralPath $executable -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $(Split-Path -Leaf $archive)" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Output "已打包 $archive；執行檔 $size 位元組"
} finally {
    Pop-Location
}
