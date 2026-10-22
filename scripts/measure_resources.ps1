param([string]$OutputPath = 'target/resource-baseline.json')
$ErrorActionPreference = 'Stop'
$workspacePath = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $workspacePath
try {
    $outputFile = [System.IO.Path]::GetFullPath($OutputPath)
    $outputDirectory = Split-Path -Parent $outputFile
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
    $stdoutPath = Join-Path $outputDirectory 'resource-benchmark.stdout.log'
    $stderrPath = Join-Path $outputDirectory 'resource-benchmark.stderr.log'
    $started = Get-Date
    $process = Start-Process -FilePath 'cargo' -ArgumentList @('test','-p','app-ui','--lib','--release','--locked','--offline','resource_baseline','--','--ignored','--nocapture') -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $process.Handle | Out-Null
    $peakWorkingSet = 0L
    $cpuSeconds = 0.0
    while (-not $process.HasExited) {
        Get-Process -Name 'app_ui-*' -ErrorAction SilentlyContinue | ForEach-Object {
            try {
                if ($_.Path.StartsWith((Join-Path $workspacePath 'target'), [StringComparison]::OrdinalIgnoreCase)) {
                    $peakWorkingSet = [Math]::Max($peakWorkingSet, $_.PeakWorkingSet64)
                    $cpuSeconds = [Math]::Max($cpuSeconds, $_.CPU)
                }
            } catch { }
        }
        Start-Sleep -Milliseconds 50
        $process.Refresh()
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { Get-Content -LiteralPath $stderrPath -Tail 20; throw '資源量測未完成' }
    $samples = @(Get-Content -LiteralPath $stdoutPath -Encoding UTF8 | Where-Object { $_.StartsWith('RESOURCE_BASELINE ') } | ForEach-Object { $_.Substring(18) | ConvertFrom-Json })
    if ($samples.Count -ne 6) { throw '量測樣本數不符' }
    $report = [ordered]@{
        measured_at = $started.ToString('o')
        rustc = (rustc --version | Out-String).Trim()
        cpu = @((Get-CimInstance Win32_Processor).Name)
        ram_bytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
        gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
        test_process_peak_working_set_bytes = $peakWorkingSet
        test_process_cpu_seconds = $cpuSeconds
        method = '同次測試、固定 PNG 測資；全圖 GPU 上限策略與視口分級對照。解碼為暖檔案快取；首次紋理為 egui CPU 交付，未量測真實 GPU 呈現。工作集含測資建立與所有案例。'
        native_cold_start_ms = $null
        native_gpu_frame_ms = $null
        samples = $samples
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $outputFile -Encoding UTF8
    Write-Output "已儲存量測：$outputFile"
} finally { Pop-Location }
