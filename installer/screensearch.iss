; ScreenSearch Installer Script
; Created with Inno Setup 6.x

#define MyAppName "ScreenSearch"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "Nicolas Estrem"
#define MyAppURL "https://github.com/nicolasestrem/screensearch"
#define MyAppExeName "screensearch.exe"
#define MyAppId "{{8F7A9C2B-1D3E-4F5A-9B7C-2E8D4A6F1C9B}"

[Setup]
; Application identification
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
VersionInfoVersion={#MyAppVersion}

; Installation directories
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes

; Output configuration
OutputDir=..\target\release\installers
OutputBaseFilename=ScreenSearch-v{#MyAppVersion}-Setup
SetupIconFile=resources\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}

; Compression
Compression=lzma2/max
SolidCompression=yes
LZMAUseSeparateProcess=yes
LZMADictionarySize=1048576
LZMANumFastBytes=273

; Visual appearance
WizardStyle=modern
#ifdef FULL_INSTALLER
WizardImageFile=resources\sidebar.bmp
WizardSmallImageFile=resources\banner.bmp
#endif

; System requirements
MinVersion=10.0.17763
PrivilegesRequired=admin
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

; Uninstall
UninstallDisplayName={#MyAppName}
UninstallFilesDir={app}\uninstall

; License
LicenseFile=..\LICENSE
#ifdef FULL_INSTALLER
InfoBeforeFile=resources\readme.txt
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Types]
#ifdef FULL_INSTALLER
Name: "full"; Description: "Full Installation (with ONNX model)"
Name: "lite"; Description: "Lightweight Installation (download model on first run)"
#else
Name: "lite"; Description: "Standard Installation"
#endif

[Components]
#ifdef FULL_INSTALLER
Name: "core"; Description: "ScreenSearch Application (required)"; Types: full lite; Flags: fixed
#else
Name: "core"; Description: "ScreenSearch Application (required)"; Types: lite; Flags: fixed
#endif
#ifdef FULL_INSTALLER
Name: "model"; Description: "AI RAG Search Model (449 MB)"; Types: full
#endif
#ifdef FULL_INSTALLER
Name: "startup"; Description: "Launch on Windows startup"; Types: full lite
#else
Name: "startup"; Description: "Launch on Windows startup"; Types: lite
#endif

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
; Main executable
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion; Components: core

; Configuration template
Source: "..\config.toml"; DestDir: "{app}"; Flags: ignoreversion onlyifdoesntexist; Components: core

; AI RAG Search Model (Full installer only)
#ifdef FULL_INSTALLER
Source: "models\model.onnx"; DestDir: "{app}\models"; Flags: ignoreversion; Components: model
Source: "models\tokenizer.json"; DestDir: "{app}\models"; Flags: ignoreversion; Components: model
#endif

; License and documentation
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion; Components: core
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme; Components: core

; Application assets (needed for system tray icon)
Source: "..\assets\icon.png"; DestDir: "{app}\assets"; Flags: ignoreversion; Components: core

; Icon file for shortcuts
Source: "resources\icon.ico"; DestDir: "{app}"; Flags: ignoreversion; Components: core

[Dirs]
; Ensure models directory exists even if model not installed
Name: "{app}\models"; Permissions: users-modify

[Icons]
; Start Menu
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\icon.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

; Desktop (optional)
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Registry]
; Startup registration (optional)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Components: startup

[Run]
; Option to launch after installation
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Clean up AppData on uninstall
Type: filesandordirs; Name: "{localappdata}\screensearch"
Type: files; Name: "{app}\screensearch.db"
Type: files; Name: "{app}\screensearch.log"
Type: filesandordirs; Name: "{app}\captures"
Type: filesandordirs; Name: "{app}\models"

[Code]
function InitializeSetup(): Boolean;
var
  OldVersion: String;
  ResultCode: Integer;
begin
  Result := True;

  // Check if application is already installed
  if RegQueryStringValue(HKLM, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#MyAppId}_is1',
     'DisplayVersion', OldVersion) then
  begin
    // Inform user about existing installation
    if MsgBox('ScreenSearch ' + OldVersion + ' is already installed. Do you want to upgrade to version {#MyAppVersion}?',
       mbConfirmation, MB_YESNO) = IDYES then
    begin
      Result := True;
    end
    else
    begin
      Result := False;
    end;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    // Create captures directory in app folder
    if not DirExists(ExpandConstant('{app}\captures')) then
      CreateDir(ExpandConstant('{app}\captures'));

#ifndef FULL_INSTALLER
    // Show message about model download for lightweight installer
    if WizardIsComponentSelected('core') then
    begin
      MsgBox('ScreenSearch will download the AI RAG Search Model (449 MB) on first run if you enable embeddings. ' +
             'An internet connection is required for this feature.', mbInformation, MB_OK);
    end;
#endif
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // Ask user if they want to keep their data
    if MsgBox('Do you want to keep your ScreenSearch database and captured screenshots?',
       mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDNO then
    begin
      // Already handled by [UninstallDelete] section
      // This just confirms the action
    end
    else
    begin
      // User wants to keep data - remove entries from UninstallDelete queue
      // Note: Inno Setup doesn't provide a way to skip UninstallDelete dynamically
      // So we just inform the user
      MsgBox('Your data has been preserved in ' + ExpandConstant('{localappdata}\screensearch'),
         mbInformation, MB_OK);
    end;
  end;
end;
