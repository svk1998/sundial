; Inno Setup script for Sundial. Built by the release workflow:
;   iscc /DAppVersion=1.2.3 /DSourceExe=path\to\sundial.exe installer\sundial.iss
; Per-user install: no admin prompt, lands in %LOCALAPPDATA%\Programs\Sundial.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\sundial.exe"
#endif

[Setup]
; Never change AppId: it is how upgrades and the uninstaller find the existing install.
AppId={{6F1C2D3E-8B7A-4C5D-9E0F-5A1B2C3D4E5F}
AppName=Sundial
AppVersion={#AppVersion}
AppPublisher=svk1998
AppPublisherURL=https://github.com/svk1998/sundial
DefaultDirName={autopf}\Sundial
DefaultGroupName=Sundial
DisableProgramGroupPage=yes
DisableDirPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=sundial-v{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\sundial.ico
UninstallDisplayName=Sundial
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; Sundial is a tray app the Restart Manager can't close cleanly; [Code] stops it instead.
CloseApplications=no

[Tasks]
Name: autostart; Description: "Start Sundial when I sign in to Windows"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "sundial.exe"; Flags: ignoreversion
Source: "..\assets\icon.ico"; DestDir: "{app}"; DestName: "sundial.ico"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Sundial"; Filename: "{app}\sundial.exe"; IconFilename: "{app}\sundial.ico"

[Registry]
; Same value the app's own "Start with Windows" toggle writes (src/platform/autostart.rs),
; so the tray checkbox shows it as ticked.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
  ValueName: "Sundial"; ValueData: """{app}\sundial.exe"""; Tasks: autostart

[Run]
Filename: "{app}\sundial.exe"; Description: "Launch Sundial"; Flags: nowait postinstall

[Code]
// Stops a running Sundial so its exe can be replaced or removed. Asks it to close
// first, then forces it; at worst that loses the last <30s of unsaved time.
procedure StopSundial();
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/im sundial.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(1000);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im sundial.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopSundial();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopSundial();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    // Removed unconditionally: the user may have turned autostart on from the
    // tray menu after installing without the task.
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Sundial');
  if CurUninstallStep = usPostUninstall then
    if SuppressibleMsgBox('Also delete your tracked time history?' + #13#10#13#10 +
        ExpandConstant('{userappdata}\Sundial'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES then
      DelTree(ExpandConstant('{userappdata}\Sundial'), True, True, True);
end;
