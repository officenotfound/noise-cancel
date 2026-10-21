# Times every model on the bundled clips and prints a summary. Run from this folder:
#   powershell -ExecutionPolicy Bypass -File .\run.ps1
# For the realistic worst case, run it ON BATTERY with the power mode set to "Best power efficiency",
# and again plugged in. Close other heavy apps. Send the printed table (or results.csv) back.
$ErrorActionPreference = "Stop"
$rows = @()
foreach ($wav in Get-ChildItem .\audio\*.wav) {
  foreach ($m in "rnnoise", "dfn3", "dfn3-ll", "fe-t", "fe-b", "fe-s", "fe-m", "fe-l") {
    $j = .\qm-bench.exe --model $m --in $wav.FullName --models-dir .\models --json | ConvertFrom-Json
    $rows += [pscustomobject]@{
      model = $m; clip = $wav.BaseName
      core_pct = [math]::Round(100 * $j.rtf, 1)
      frame_mean_ms = [math]::Round($j.frame_ms_mean, 2)
      frame_p99_ms = [math]::Round($j.frame_ms_p99, 2)
      frame_max_ms = [math]::Round($j.frame_ms_max, 2)
      hop_ms = [math]::Round($j.hop_ms, 1)
      frames_over_hop = $j.frames_over_hop
    }
  }
}
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
Write-Host "CPU: $cpu"
$rows | Format-Table -AutoSize
$rows | Export-Csv results.csv -NoTypeInformation
"CPU: $cpu" | Out-File results.txt
$rows | Format-Table -AutoSize | Out-String | Out-File results.txt -Append
