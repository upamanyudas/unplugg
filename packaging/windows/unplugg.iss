; Windows installer:  iscc -DAppVersion=1.0.0 packaging\windows\unplugg.iss
; Wraps dist\unplugg (packaging\build.py) into dist\unplugg-windows-x64-setup.exe.
; Per-user, so no admin prompt, and the in-app updater can rerun it silently:
; /VERYSILENT closes the running copy, replaces it, and starts the new one.

[Setup]
AppId={{51117B94-65BE-4934-85A2-E302EEAC19A0}
AppName=unplugg
AppVersion={#AppVersion}
AppPublisher=Upa Das
AppPublisherURL=https://github.com/upamanyudas/unplugg
AppCopyright=Copyright (C) 2025 Upa Das. MIT License.
PrivilegesRequired=lowest
DefaultDirName={autopf}\unplugg
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SetupIconFile=..\..\assets\logo.ico
UninstallDisplayIcon={app}\unplugg.exe
WizardStyle=modern
OutputDir=..\..\dist
OutputBaseFilename=unplugg-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes

[Files]
Source: "..\..\dist\unplugg\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{autoprograms}\unplugg"; Filename: "{app}\unplugg.exe"

[Registry]
; unplugg writes its own launch-at-login value; only remove it on uninstall
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "unplugg"; Flags: uninsdeletevalue dontcreatekey

[Run]
; no skipifsilent: a silent update starts the new copy too
Filename: "{app}\unplugg.exe"; Description: "Open unplugg"; Flags: nowait postinstall
