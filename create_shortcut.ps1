$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("C:\Users\aleja\Downloads\Conduit-2026-11-03\Fast Build Conduit.lnk")
$Shortcut.TargetPath = "C:\Users\aleja\Downloads\Conduit-2026-11-03\build.bat"
$Shortcut.WorkingDirectory = "C:\Users\aleja\Downloads\Conduit-2026-11-03"
$Shortcut.Description = "Fast Parallel Build for Conduit (16 Threads)"
$Shortcut.Save()
