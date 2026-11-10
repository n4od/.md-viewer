<#
  Generates the icon set in src-tauri/icons from scratch with GDI+.
  No design tools or extra CLI packages required.

  Usage:  powershell -ExecutionPolicy Bypass -File scripts\make-icons.ps1
#>
[CmdletBinding()]
param(
    [string]$OutDir
)

$scriptRoot = $PSScriptRoot
if ([string]::IsNullOrEmpty($scriptRoot)) {
    $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
}
if ([string]::IsNullOrEmpty($OutDir)) {
    $OutDir = Join-Path $scriptRoot '..\src-tauri\icons'
}

Add-Type -AssemblyName System.Drawing

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$bg      = [System.Drawing.Color]::FromArgb(255, 22, 26, 32)
$ink     = [System.Drawing.Color]::FromArgb(255, 230, 235, 242)
$accent  = [System.Drawing.Color]::FromArgb(255, 116, 203, 189)

function New-MarkBitmap {
    param([int]$Size)

    $bmp = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.Clear([System.Drawing.Color]::Transparent)

    # Rounded square background.
    $pad = [Math]::Max(1, [int]($Size * 0.03))
    $rect = New-Object System.Drawing.Rectangle($pad, $pad, ($Size - 2 * $pad), ($Size - 2 * $pad))
    $radius = [int]($Size * 0.22)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
    $path.AddArc(($rect.Right - $d), $rect.Y, $d, $d, 270, 90)
    $path.AddArc(($rect.Right - $d), ($rect.Bottom - $d), $d, $d, 0, 90)
    $path.AddArc($rect.X, ($rect.Bottom - $d), $d, $d, 90, 90)
    $path.CloseFigure()
    $brush = New-Object System.Drawing.SolidBrush($bg)
    $g.FillPath($brush, $path)

    # Serif "M", optically centred.
    $family = $null
    foreach ($name in @('Iowan Old Style', 'Georgia', 'Times New Roman', 'DejaVu Serif', 'serif')) {
        try {
            $family = New-Object System.Drawing.FontFamily($name)
            if ($family -ne $null) { break }
        } catch { $family = $null }
    }
    if ($family -eq $null) { $family = [System.Drawing.FontFamily]::GenericSerif }

    $fontSize = [single]($Size * 0.66)
    $font = New-Object System.Drawing.Font($family, $fontSize, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $format.LineAlignment = [System.Drawing.StringAlignment]::Center
    $inkBrush = New-Object System.Drawing.SolidBrush($ink)
    $textRect = New-Object System.Drawing.RectangleF(0, [single]($Size * 0.02), [single]$Size, [single]$Size)
    $g.DrawString('M', $font, $inkBrush, $textRect, $format)

    # Accent caret, the "cursor" from the welcome screen.
    if ($Size -ge 32) {
        $w = [single]($Size * 0.05)
        $h = [single]($Size * 0.24)
        $x = [single]($Size * 0.71)
        $y = [single]($Size * 0.44)
        $accentBrush = New-Object System.Drawing.SolidBrush($accent)
        $g.FillRectangle($accentBrush, $x, $y, $w, $h)
        $accentBrush.Dispose()
    }

    $inkBrush.Dispose()
    $brush.Dispose()
    $font.Dispose()
    $format.Dispose()
    $g.Dispose()
    return $bmp
}

function Save-Png {
    param([System.Drawing.Bitmap]$Bitmap, [string]$Path, [int]$Size)
    $scaled = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($scaled)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($Bitmap, 0, 0, $Size, $Size)
    $g.Dispose()
    $scaled.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $scaled.Dispose()
}

function New-PngBytes {
    param([System.Drawing.Bitmap]$Bitmap, [int]$Size)
    $stream = New-Object System.IO.MemoryStream
    $scaled = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($scaled)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($Bitmap, 0, 0, $Size, $Size)
    $g.Dispose()
    $scaled.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $scaled.Dispose()
    $bytes = $stream.ToArray()
    $stream.Dispose()
    return $bytes
}

# Master artwork, rendered at 256 and scaled down.
$master = New-MarkBitmap -Size 256

Save-Png -Bitmap $master -Path (Join-Path $OutDir 'icon.png') -Size 512
Save-Png -Bitmap $master -Path (Join-Path $OutDir '32x32.png') -Size 32
Save-Png -Bitmap $master -Path (Join-Path $OutDir '128x128.png') -Size 128
Save-Png -Bitmap $master -Path (Join-Path $OutDir '128x128@2x.png') -Size 256
Save-Png -Bitmap $master -Path (Join-Path $OutDir 'icon-512.png') -Size 512

# Multi-resolution .ico (PNG-compressed entries, Vista and newer).
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$images = @()
foreach ($size in $sizes) {
    $bytes = New-PngBytes -Bitmap $master -Size $size
    $images += [pscustomobject]@{ Size = $size; Bytes = [byte[]]$bytes }
}

$iconPath = Join-Path $OutDir 'icon.ico'
$file = [System.IO.File]::Create($iconPath)
$writer = New-Object System.IO.BinaryWriter($file)

# ICONDIR
$writer.Write([UInt16]0)                 # reserved
$writer.Write([UInt16]1)                 # type: icon
$writer.Write([UInt16]$images.Count)     # image count

$offset = 6 + (16 * $images.Count)
foreach ($image in $images) {
    $dim = 0
    if ($image.Size -lt 256) { $dim = $image.Size }
    $writer.Write([Byte]$dim)                    # width (0 means 256)
    $writer.Write([Byte]$dim)                    # height (0 means 256)
    $writer.Write([Byte]0)                       # palette count
    $writer.Write([Byte]0)                       # reserved
    $writer.Write([UInt16]1)                     # colour planes
    $writer.Write([UInt16]32)                    # bits per pixel
    $writer.Write([UInt32]$image.Bytes.Length)   # size of image data
    $writer.Write([UInt32]$offset)               # offset of image data
    $offset += $image.Bytes.Length
}
foreach ($image in $images) {
    $writer.Write($image.Bytes)
}

$writer.Flush()
$writer.Dispose()
$file.Dispose()
Write-Host ("icon.ico bytes: " + (Get-Item $iconPath).Length)
$master.Dispose()

Write-Host "Icons written to $OutDir"
Get-ChildItem $OutDir | Select-Object Name, Length | Format-Table -AutoSize
