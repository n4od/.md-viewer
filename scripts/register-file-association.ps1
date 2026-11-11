<#
  Registers MD Viewer for .md (and friends) in the *current user* hive, which
  needs no administrator rights. Meant for testing an unpackaged build; the
  NSIS installer that `npm run bundle` produces does the same thing machine-wide.

  Usage:
    powershell -ExecutionPolicy Bypass -File scripts\register-file-association.ps1
    powershell -ExecutionPolicy Bypass -File scripts\register-file-association.ps1 -SetDefault

  -SetDefault also points the extension's default ProgId at MD Viewer. Windows
  10/11 still shows a "How do you want to open this file?" prompt the first time
  if the user previously chose another app; pick MD Viewer and tick "Always".
#>
[CmdletBinding()]
param(
    [string]$ExePath,
    [switch]$SetDefault
)

$ErrorActionPreference = 'Stop'

$scriptRoot = $PSScriptRoot
if ([string]::IsNullOrEmpty($scriptRoot)) {
    $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
}
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot '..'))

if ([string]::IsNullOrEmpty($ExePath)) {
    $candidates = @(
        (Join-Path $repoRoot 'src-tauri\target\release\mdviewer.exe'),
        (Join-Path $repoRoot 'src-tauri\target\debug\mdviewer.exe')
    )
    $ExePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if ([string]::IsNullOrEmpty($ExePath) -or -not (Test-Path $ExePath)) {
    throw "Could not find mdviewer.exe. Build it first (cargo build --release) or pass -ExePath."
}
$ExePath = [System.IO.Path]::GetFullPath($ExePath)

$progId      = 'MDViewer.Markdown'
$appId       = 'mdviewer.exe'
$description = 'Markdown Document'
$extensions  = @('.md', '.markdown', '.mdown', '.mkd', '.mkdn', '.mdwn', '.mdtxt', '.mdtext')

function New-Key([string]$Path) {
    if (-not (Test-Path $Path)) { New-Item -Path $Path -Force | Out-Null }
}

# --- ProgId: what the type is, its icon, and how to open it -----------------
New-Key "HKCU:\Software\Classes\$progId"
Set-ItemProperty "HKCU:\Software\Classes\$progId" -Name '(Default)' -Value $description

# Explorer draws this icon for every Markdown file, so it is the document artwork
# rather than the executable's. Written beside the app's other per-user state, and
# falling back to the executable's own icon if that cannot be done.
$iconDir  = Join-Path $env:LOCALAPPDATA 'mdviewer'
$iconPath = Join-Path $iconDir 'markdown.ico'
$iconSrc  = Join-Path $PSScriptRoot '..\src-tauri\icons\markdown.ico'
if (Test-Path $iconSrc) {
    New-Item -ItemType Directory -Force -Path $iconDir | Out-Null
    Copy-Item $iconSrc $iconPath -Force
}
$iconValue = "`"$ExePath`",0"
if (Test-Path $iconPath) { $iconValue = "`"$iconPath`",0" }

New-Key "HKCU:\Software\Classes\$progId\DefaultIcon"
Set-ItemProperty "HKCU:\Software\Classes\$progId\DefaultIcon" -Name '(Default)' -Value $iconValue

New-Key "HKCU:\Software\Classes\$progId\shell\open\command"
Set-ItemProperty "HKCU:\Software\Classes\$progId\shell\open\command" -Name '(Default)' -Value "`"$ExePath`" `"%1`""

# --- Per-extension: offer MD Viewer in "Open with" and (optionally) default ---
foreach ($ext in $extensions) {
    New-Key "HKCU:\Software\Classes\$ext"
    New-Key "HKCU:\Software\Classes\$ext\OpenWithProgids"
    Set-ItemProperty "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name $progId -Value ([byte[]]@()) -Type Binary

    if ($SetDefault) {
        Set-ItemProperty "HKCU:\Software\Classes\$ext" -Name '(Default)' -Value $progId
    }
}

# --- Applications\mdviewer.exe: this is what "Open with" enumerates ----------
# The shell reads HKCR\Applications\<exe>. HKCU\Software\Classes is part of HKCR,
# so a per-user registration is enough. FriendlyAppName is what the picker shows
# as the app's label; without it the entry is listed with no name (or filtered).
New-Key "HKCU:\Software\Classes\Applications\$appId"
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId" -Name '(Default)' -Value 'MD Viewer'
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId" -Name 'FriendlyAppName' -Value 'MD Viewer'
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId" -Name 'ApplicationName' -Value 'MD Viewer'
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId" -Name 'ApplicationCompany' -Value 'MD Viewer'
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId" -Name 'ApplicationIcon' -Value "`"$ExePath`",0"

New-Key "HKCU:\Software\Classes\Applications\$appId\DefaultIcon"
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId\DefaultIcon" -Name '(Default)' -Value "`"$ExePath`",0"

New-Key "HKCU:\Software\Classes\Applications\$appId\shell\open\command"
Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId\shell\open\command" -Name '(Default)' -Value "`"$ExePath`" `"%1`""
New-Key "HKCU:\Software\Classes\Applications\$appId\SupportedTypes"
foreach ($ext in $extensions) {
    Set-ItemProperty "HKCU:\Software\Classes\Applications\$appId\SupportedTypes" -Name $ext -Value ''
}

# --- Explorer's own MRU: without this, the picker only shows it under "more
# apps". Merge into the existing letter-keyed list instead of overwriting it, so
# other apps' "Open with" order survives.
$exeName = Split-Path -Leaf $ExePath
foreach ($ext in $extensions) {
    $openWithList = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext\OpenWithList"
    New-Key $openWithList

    $existing = @{}
    foreach ($name in @((Get-Item $openWithList).Property)) {
        if ($name -match '^PS' -or $name -eq 'MRUList') { continue }
        $existing[$name] = (Get-ItemProperty -Path $openWithList -Name $name -ErrorAction SilentlyContinue).$name
    }

    $letter = $existing.Keys | Where-Object { $existing[$_] -eq $exeName } | Select-Object -First 1
    if (-not $letter) {
        # Range is integer-only in PowerShell: 'a'..'z' throws, chars must be built.
        $letters = 0..25 | ForEach-Object { [string][char](97 + $_) }
        $letter = $letters | Where-Object { -not $existing.ContainsKey($_) } | Select-Object -First 1
    }

    if ($letter) {
        Set-ItemProperty $openWithList -Name $letter -Value $exeName

        $order = @($letter)
        $previous = (Get-ItemProperty -Path $openWithList -Name 'MRUList' -ErrorAction SilentlyContinue).MRUList
        if ($previous) {
            foreach ($character in $previous.ToCharArray()) {
                $key = [string]$character
                if ($existing.ContainsKey($key) -and ($order -notcontains $key)) { $order += $key }
            }
        }
        # Entries that exist but were missing from MRUList still belong in it.
        foreach ($name in ($existing.Keys | Sort-Object)) {
            if ($order -notcontains $name) { $order += $name }
        }
        Set-ItemProperty $openWithList -Name 'MRUList' -Value ($order -join '')
    }
}

New-Key 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts'
foreach ($ext in $extensions) {
    Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts' `
        -Name ("{0}_{1}" -f $exeName, $ext) -Value 0 -Type DWord
}

# Tell the shell something changed so Explorer picks it up without a reboot.
try {
    Add-Type -Namespace Win32 -Name Shell -MemberDefinition @'
[DllImport("shell32.dll")] public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);
'@ -ErrorAction SilentlyContinue
    [Win32.Shell]::SHChangeNotify(0x08000000, 0x0000, [IntPtr]::Zero, [IntPtr]::Zero)
} catch { }

Write-Host "Registered '$progId' -> $ExePath"
Write-Host "Extensions: $($extensions -join ', ')"
if (-not $SetDefault) {
    Write-Host 'Tip: re-run with -SetDefault to make MD Viewer the default handler.'
}
Write-Host 'Then: right-click a .md file -> Open with -> Choose another app -> MD Viewer -> Always.'
