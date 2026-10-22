# Before start-Mokora.bat starts Mokora: is anything from this folder still
# running — an Mokora (any account's), its game browser, or the frontend dev
# server? Starting again on top of them fights over the browser profile and
# the dev server's port, so list them and close them once the user agrees
# (y / Y). Only processes whose path or command line points into this folder
# are touched: other Brave / Chrome / Node windows are left alone.
#
# Exit code: 0 = nothing left running (start), 1 = the user kept them (don't).
#
# With -DevServer it only closes this folder's frontend dev server, without
# asking: start-Mokora.bat does that once Mokora has exited, so the dev server
# it started does not keep the console window open.
param(
    [Parameter(Mandatory = $true)][string]$Root,
    [switch]$DevServer
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $Root).Path.TrimEnd('\')
$names = 'mokora.exe', 'brave.exe', 'chrome.exe', 'msedge.exe', 'node.exe'

function Find-Leftovers {
    @(Get-CimInstance Win32_Process | Where-Object {
        $names -contains $_.Name.ToLower() -and (
            ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) -or
            ($_.CommandLine -and $_.CommandLine.IndexOf($root, [StringComparison]::OrdinalIgnoreCase) -ge 0))
    })
}

if ($DevServer) {
    foreach ($p in @(Find-Leftovers | Where-Object { $_.Name -ieq 'node.exe' })) {
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }
    exit 0
}

$found = Find-Leftovers
if ($found.Count -eq 0) {
    Write-Host '沒有正在執行的 Mokora，直接啟動。'
    exit 0
}

$mokora = @($found | Where-Object { $_.Name -ieq 'mokora.exe' })
$node = @($found | Where-Object { $_.Name -ieq 'node.exe' })
$browser = @($found | Where-Object { $_.Name -ine 'mokora.exe' -and $_.Name -ine 'node.exe' })

Write-Host ''
Write-Host '偵測到這個資料夾還有程式在執行：' -ForegroundColor Yellow
foreach ($p in $mokora) {
    # No --config: the one started from the console, on the startup account.
    $account = '主控台'
    if ($p.CommandLine -match 'accounts\\(\d+)\\config\.toml') { $account = "帳號 $($Matches[1])" }
    Write-Host "  Mokora（$account）  PID $($p.ProcessId)"
}
if ($browser.Count -gt 0) { Write-Host "  遊戲瀏覽器：$($browser.Count) 個程序" }
if ($node.Count -gt 0) { Write-Host "  前端伺服器：$($node.Count) 個程序" }
Write-Host ''

$answer = Read-Host '要把它們全部關閉再啟動嗎？輸入 y 或 Y 同意'
if ($answer -cne 'y' -and $answer -cne 'Y') {
    Write-Host '沒有關閉，這次不啟動。' -ForegroundColor Yellow
    exit 1
}

# Mokora first, so none of them opens a browser again; then the rest.
foreach ($group in @($mokora, $browser, $node)) {
    foreach ($p in $group) {
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }
}
Start-Sleep -Seconds 2

$left = Find-Leftovers
if ($left.Count -gt 0) {
    Write-Host "還有 $($left.Count) 個程序關不掉，請手動關閉後再啟動。" -ForegroundColor Red
    exit 1
}
Write-Host '已全部關閉。' -ForegroundColor Green
exit 0
