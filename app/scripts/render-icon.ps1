param([string]$Out = "$PSScriptRoot\..\app-icon.png")

Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = "AntiAlias"
$g.Clear([System.Drawing.Color]::Transparent)

# Rounded square tile with a dark vertical gradient.
$radius = 220
$rect = New-Object System.Drawing.Rectangle 40, 40, ($size - 80), ($size - 80)
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$d = $radius * 2
$path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
$path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
$path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
$path.CloseFigure()

$tile = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect,
    ([System.Drawing.Color]::FromArgb(255, 24, 34, 48)),
    ([System.Drawing.Color]::FromArgb(255, 10, 14, 20)), 90
$g.FillPath($tile, $path)

$edge = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 36, 48, 64)), 6
$g.DrawPath($edge, $path)

# Four-point spark in cyan.
$cx = $size / 2
$cy = $size / 2
$long = 300
$short = 64
$points = @(
    (New-Object System.Drawing.PointF $cx, ($cy - $long)),
    (New-Object System.Drawing.PointF ($cx + $short), ($cy - $short)),
    (New-Object System.Drawing.PointF ($cx + $long), $cy),
    (New-Object System.Drawing.PointF ($cx + $short), ($cy + $short)),
    (New-Object System.Drawing.PointF $cx, ($cy + $long)),
    (New-Object System.Drawing.PointF ($cx - $short), ($cy + $short)),
    (New-Object System.Drawing.PointF ($cx - $long), $cy),
    (New-Object System.Drawing.PointF ($cx - $short), ($cy - $short))
)
$spark = New-Object System.Drawing.Drawing2D.LinearGradientBrush ([System.Drawing.PointF]::new($cx, $cy - $long)), ([System.Drawing.PointF]::new($cx, $cy + $long)),
    ([System.Drawing.Color]::FromArgb(255, 125, 224, 255)),
    ([System.Drawing.Color]::FromArgb(255, 14, 165, 233))
$g.FillPolygon($spark, [System.Drawing.PointF[]]$points)

# Small companion spark.
$s = 90
$ox = $cx + 250
$oy = $cy - 250
$small = @(
    (New-Object System.Drawing.PointF $ox, ($oy - $s)),
    (New-Object System.Drawing.PointF ($ox + 20), ($oy - 20)),
    (New-Object System.Drawing.PointF ($ox + $s), $oy),
    (New-Object System.Drawing.PointF ($ox + 20), ($oy + 20)),
    (New-Object System.Drawing.PointF $ox, ($oy + $s)),
    (New-Object System.Drawing.PointF ($ox - 20), ($oy + 20)),
    (New-Object System.Drawing.PointF ($ox - $s), $oy),
    (New-Object System.Drawing.PointF ($ox - 20), ($oy - 20))
)
$g.FillPolygon((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 186, 238, 255))), [System.Drawing.PointF[]]$small)

$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "Wrote $Out"
