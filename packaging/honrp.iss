; honrp — per-user Windows installer (Inno Setup 6)
;
; Build:  ISCC.exe /DAppVersion=0.3.12 packaging\honrp.iss
; Output: dist\honrp-setup-0.3.12.exe
;
; Installs into %LOCALAPPDATA%\Programs\honrp (no admin prompt), creates a
; Start Menu shortcut, optionally starts with Windows, and offers to launch
; the control window at the end.  The Lua bridge is written by honrp itself
; on first start.

#ifndef AppVersion
  #define AppVersion "0.3.12"
#endif

#define AppName "honrp"
#define AppPublisher "NasirA1991"
#define AppURL "https://github.com/NasirA1991/honrp"
#define AppExeName "honrp.exe"

[Setup]
AppId={{8E2C0A2F-6C79-4E4B-9A57-3C5D038B7A11}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
SetupIconFile=..\assets\icon.ico
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=honrp-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
Name: "autostart"; Description: "Start {#AppName} when I sign in to Windows"; GroupDescription: "Startup:"; Flags: checkedonce

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README-WINDOWS.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\config.example.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\{#AppExeName}"; IconIndex: 0; Comment: "Heroes of Newerth Rich Presence"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\{#AppExeName}"; IconIndex: 0; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "honrp"; ValueData: """{app}\{#AppExeName}"" run"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Start {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\honrp"

[UninstallRun]
; stop the background daemon before removing files
Filename: "{app}\{#AppExeName}"; Parameters: "stop"; Flags: runhidden; RunOnceId: "StopDaemon"
; strip the Lua bridge from the game's startup.lua (keeps any user content)
Filename: "{app}\{#AppExeName}"; Parameters: "uninstall"; Flags: runhidden; RunOnceId: "RemoveBridge"

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  { an upgrade must stop the old daemon before its exe is replaced }
  Exec(ExpandConstant('{app}\{#AppExeName}'), 'stop', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;
