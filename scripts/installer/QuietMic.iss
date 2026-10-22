; Inno Setup script. Build with: ISCC scripts\installer\QuietMic.iss  (after scripts/package_windows.sh)
; Per-user install (no administrator needed). VB-Audio Virtual Cable is a separate, free driver (not bundled).

#define AppVersion "0.1.0"

[Setup]
AppId={{6F0B2E0A-5A55-4C1B-9D2E-3C3E7B1A0001}
AppName=QuietMic
AppVersion={#AppVersion}
AppPublisher=QuietMic
DefaultDirName={autopf}\QuietMic
DefaultGroupName=QuietMic
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=QuietMic-Setup
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\QuietMic.exe
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "autostart"; Description: "Start QuietMic when Windows starts"; Flags: unchecked

[Files]
Source: "..\..\dist\QuietMic\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\QuietMic"; Filename: "{app}\QuietMic.exe"
Name: "{autodesktop}\QuietMic"; Filename: "{app}\QuietMic.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "QuietMic"; ValueData: """{app}\QuietMic.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "https://vb-audio.com/Cable/"; Description: "Open the VB-Cable download page (the virtual microphone, needed once)"; Flags: shellexec postinstall skipifsilent unchecked
Filename: "{app}\QuietMic.exe"; Description: "Start QuietMic now"; Flags: nowait postinstall skipifsilent
