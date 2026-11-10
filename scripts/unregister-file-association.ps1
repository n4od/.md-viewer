<#
  Undoes scripts\register-file-association.ps1 *and* the in-app "Set as default"
  action. Removes only entries that belong to MD Viewer, so existing apps in the
  "Open with" list are left alone.

  Usage: powershell -ExecutionPolicy Bypass -File scripts\unregister-file-association.ps1
         ... -Force     also clears the extension's default value if MD Viewer set it
#>
[CmdletBinding()]
param([switch]$Force)

$ErrorActionPreference = 'Stop'

$progId     = 'MDViewer.Markdown'
$exePattern = 'mdviewer*.exe'
$extensions = @('.md', '.markdown', '.mdown', '.mkd', '.mkdn', '.mdwn', '.mdtxt', '.mdtext')

function Remove-Key([string]$Path) {
    if (Test-Path $Path) { Remove-Item -Path $Path -Recurse -Force -ErrorAction SilentlyContinue }
}

function Test-OurExe([string]$Name) {
    return $Name -like $exePattern
}

# --- ProgId and Applications\mdviewer*.exe ---------------------------------
Remove-Key "HKCU:\Software\Classes\$progId"

$applications = 'HKCU:\Software\Classes\Applications'
if (Test-Path $applications) {
    Get-ChildItem $applications -ErrorAction SilentlyContinue |
        Where-Object { Test-OurExe $_.PSChildName } |
        ForEach-Object { Remove-Key $_.PSPath }
}

# --- Per-extension: OpenWithProgids, Explorer MRU --------------------------
foreach ($ext in $extensions) {
    $openWith = "HKCU:\Software\Classes\$ext\OpenWithProgids"
    if (Test-Path $openWith) {
        Remove-ItemProperty -Path $openWith -Name $progId -ErrorAction SilentlyContinue
        $values = (Get-Item $openWith).Property | Where-Object { $_ -notmatch '^PS' }
        $children = Get-ChildItem $openWith -ErrorAction SilentlyContinue
        if (-not $children -and -not $values) { Remove-Key $openWith }
    }

    if ($Force) {
        $classKey = "HKCU:\Software\Classes\$ext"
        if (Test-Path $classKey) {
            $current = (Get-ItemProperty -Path $classKey -Name '(Default)' -ErrorAction SilentlyContinue).'(default)'
            if ($current -eq $progId) {
                Remove-ItemProperty -Path $classKey -Name '(Default)' -ErrorAction SilentlyContinue
            }
        }
    }

    # Explorer's short list: drop our exe, then rebuild MRUList from what is left.
    $openWithList = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext\OpenWithList"
    if (Test-Path $openWithList) {
        $item = Get-Item $openWithList
        foreach ($name in @($item.Property)) {
            if ($name -match '^PS' -or $name -eq 'MRUList') { continue }
            $value = (Get-ItemProperty -Path $openWithList -Name $name).$name
            if ($value -is [string] -and (Test-OurExe $value)) {
                Remove-ItemProperty -Path $openWithList -Name $name -ErrorAction SilentlyContinue
            }
        }

        $remaining = @((Get-Item $openWithList).Property | Where-Object { $_ -notmatch '^PS' -and $_ -ne 'MRUList' })
        if ($remaining.Count -gt 0) {
            Set-ItemProperty -Path $openWithList -Name 'MRUList' -Value (($remaining | Sort-Object) -join '')
        } else {
            Remove-ItemProperty -Path $openWithList -Name 'MRUList' -ErrorAction SilentlyContinue
            Remove-Key $openWithList
        }
    }
}

# --- ApplicationAssociationToasts -----------------------------------------
$toasts = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts'
if (Test-Path $toasts) {
    foreach ($name in @((Get-Item $toasts).Property)) {
        if ($name -match '^PS') { continue }
        # Windows writes both "mdviewer.exe_.md" and, after the picker is used,
        # "Applications\mdviewer.exe_.md" / "MDViewer.Markdown_.md".
        if ($name -match '(?i)mdviewer') {
            Remove-ItemProperty -Path $toasts -Name $name -ErrorAction SilentlyContinue
        }
    }
}

try {
    Add-Type -Namespace Win32 -Name Shell -MemberDefinition @'
[DllImport("shell32.dll")] public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);
'@ -ErrorAction SilentlyContinue
    [Win32.Shell]::SHChangeNotify(0x08000000, 0x0000, [IntPtr]::Zero, [IntPtr]::Zero)
} catch { }

Write-Host 'MD Viewer file associations and "Open with" entries removed for the current user.'