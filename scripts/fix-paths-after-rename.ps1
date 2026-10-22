# Run once after the project folder is renamed or moved (e.g. Mokora-0.2.1 ->
# Mokora-0.2.2):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fix-paths-after-rename.ps1
#
# What a rename leaves pointing at the old folder, and what this does about it:
#  - the desktop and Start menu shortcuts and the installed-app registry entries
#    are pointed at this folder's dist\Mokora and start-Mokora.bat;
#  - cargo's build cache under target\ records absolute paths, and a build from
#    the new place fails on them ("failed to read plugin permissions"), so the
#    cached build output is deleted (the next build is a full one).
# Nothing here touches the sources, dist\, or the bots' training data.

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$app  = Join-Path $root 'dist\Mokora'
$exe  = Join-Path $app 'mokora.exe'
$bat  = Join-Path $root 'start-Mokora.bat'

if (Get-Process mokora -ErrorAction SilentlyContinue) {
    Write-Error 'Mokora is running: close it first.'
}

# Shortcuts that belong to Mokora: by file name, wherever they are.
$sh = New-Object -ComObject WScript.Shell
$folders = @(
    [Environment]::GetFolderPath('Desktop'),
    (Join-Path $env:USERPROFILE 'Desktop'),
    (Join-Path $env:USERPROFILE 'OneDrive\Desktop'),
    (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique
foreach ($f in $folders) {
    Get-ChildItem $f -Filter '*Mokora*.lnk' -ErrorAction SilentlyContinue | ForEach-Object {
        $l = $sh.CreateShortcut($_.FullName)
        if ($l.TargetPath -like '*start-Mokora.bat') { $target = $bat; $wd = $root }
        elseif ($l.TargetPath -like '*mokora.exe')   { $target = $exe; $wd = $app }
        else { return }
        $l.TargetPath = $target
        $l.WorkingDirectory = $wd
        $l.Save()
        Write-Output "shortcut  $($_.FullName) -> $target"
    }
}

# The installer's entries (per user).
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mokora'
if (Test-Path $key) {
    Set-ItemProperty $key 'InstallLocation' "`"$app`""
    Set-ItemProperty $key 'UninstallString' "`"$(Join-Path $app 'uninstall.exe')`""
    Set-ItemProperty $key 'DisplayIcon'     "`"$exe`""
    Write-Output "registry  $key"
}
if (Test-Path 'HKCU:\Software\Koraqq\Mokora') {
    Set-ItemProperty 'HKCU:\Software\Koraqq\Mokora' '(default)' $app
    Write-Output 'registry  HKCU:\Software\Koraqq\Mokora'
}

# Stale build cache.
foreach ($profile in 'debug', 'release') {
    foreach ($d in 'build', 'deps', '.fingerprint', 'incremental', 'examples') {
        $p = Join-Path $root "target\$profile\$d"
        if (Test-Path $p) { Remove-Item $p -Recurse -Force; Write-Output "cleared   $p" }
    }
}
Write-Output 'done'
