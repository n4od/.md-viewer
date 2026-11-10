<#
  Ask Windows what it would offer and run for an extension, instead of guessing
  from the registry.

    1. SHAssocEnumHandlers  - the API the "Open with" / "Choose another app"
                              picker uses to build its list.
    2. AssocQueryString     - resolves the command Windows would execute for a
                              given ProgId, i.e. what a double-click does once
                              that ProgId is the chosen handler.

  Usage:
    powershell -ExecutionPolicy Bypass -File scripts\show-open-with.ps1
    powershell -ExecutionPolicy Bypass -File scripts\show-open-with.ps1 -Extension .txt
    powershell -ExecutionPolicy Bypass -File scripts\show-open-with.ps1 -ProgId MDViewer.Markdown
#>
[CmdletBinding()]
param(
    [string]$Extension = '.md',
    [string]$ProgId = 'MDViewer.Markdown'
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

[ComImport, Guid("973810ae-9599-4b88-9e4d-6ee98c9552da"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IEnumAssocHandlers
{
    [PreserveSig] int Next(uint celt, out IAssocHandler rgelt, out uint pceltFetched);
}

[ComImport, Guid("f04061ac-1659-4a3f-a954-775aa57fc083"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAssocHandler
{
    [PreserveSig] int GetName([MarshalAs(UnmanagedType.LPWStr)] out string ppsz);
    [PreserveSig] int GetUIName([MarshalAs(UnmanagedType.LPWStr)] out string ppsz);
    [PreserveSig] int GetIconLocation([MarshalAs(UnmanagedType.LPWStr)] out string ppsz, out int pIndex);
    [PreserveSig] int IsRecommended();
    [PreserveSig] int MakeDefault([MarshalAs(UnmanagedType.LPWStr)] string pszDescription);
    [PreserveSig] int Invoke([MarshalAs(UnmanagedType.IUnknown)] object pdo);
    [PreserveSig] int CreateInvoker([MarshalAs(UnmanagedType.IUnknown)] object pdo, [MarshalAs(UnmanagedType.IUnknown)] out object ppInvoker);
}

public static class ShellAssociations
{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    static extern void SHAssocEnumHandlers(string pszExtra, int afFilter, out IEnumAssocHandlers ppEnumHandler);

    public class Handler
    {
        public string Name;
        public string UiName;
        public bool Recommended;
    }

    public static List<Handler> List(string extension)
    {
        var result = new List<Handler>();
        IEnumAssocHandlers enumerator;
        SHAssocEnumHandlers(extension, 0, out enumerator);
        if (enumerator == null) return result;

        IAssocHandler handler;
        uint fetched;
        while (enumerator.Next(1, out handler, out fetched) == 0 && fetched == 1 && handler != null)
        {
            var item = new Handler();
            string name, uiName;
            try { handler.GetName(out name); item.Name = name; } catch { item.Name = ""; }
            try { handler.GetUIName(out uiName); item.UiName = uiName; } catch { item.UiName = ""; }
            try { item.Recommended = handler.IsRecommended() == 0; } catch { item.Recommended = false; }
            result.Add(item);
            Marshal.ReleaseComObject(handler);
        }
        Marshal.ReleaseComObject(enumerator);
        return result;
    }
}
'@

Write-Host "Apps the shell offers for '$Extension' (this is the 'Open with' list):"
Write-Host ''

$handlers = [ShellAssociations]::List($Extension)
if ($handlers.Count -eq 0) {
    Write-Host '  (none reported)'
} else {
    foreach ($handler in $handlers) {
        $label = if ($handler.UiName) { $handler.UiName } else { '(no FriendlyAppName - entry will look blank)' }
        $flag = if ($handler.Recommended) { 'recommended' } else { '' }
        Write-Host ("  {0,-52} {1,-12} {2}" -f $label, $flag, $handler.Name)
    }
}

Write-Host ''
Write-Host "Registered per-user handler (HKCU\Software\Classes\$ProgId):"
$userCommand = (Get-ItemProperty "HKCU:\Software\Classes\$ProgId\shell\open\command" -ErrorAction SilentlyContinue).'(default)'
if ($userCommand) {
    Write-Host "  command  : $userCommand"
} else {
    Write-Host '  (not registered - run scripts\register-file-association.ps1)'
}
Write-Host ''
Write-Host 'note: assoc/ftype only read the machine hive, so they stay silent for a'
Write-Host '      per-user registration even when the shell sees it fine.'

Write-Host ''
Write-Host 'Current per-user default (UserChoice). Windows prefers this over any'
Write-Host 'registry default, which is why a double-click can still open another app:'
$choice = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension\UserChoice" -ErrorAction SilentlyContinue
if ($choice -and $choice.ProgId) { Write-Host "  $($choice.ProgId)" } else { Write-Host '  (none set)' }
