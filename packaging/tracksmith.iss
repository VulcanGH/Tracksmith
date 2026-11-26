#ifndef AppVersion
  #define AppVersion "1.0.1"
#endif
[Setup]
AppId={{94106239-C4A2-434A-9792-47A87943B79D}
AppName=Tracksmith
AppVersion={#AppVersion}
AppPublisher=Tracksmith contributors
DefaultDirName={autopf}\Tracksmith
UsePreviousAppDir=no
DefaultGroupName=Tracksmith
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\artifacts\packages
OutputBaseFilename=Tracksmith-{#AppVersion}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\tracksmith.exe
SetupIconFile=tracksmith.ico
LicenseFile=..\LICENSE
VersionInfoDescription=Tracksmith — Media metadata. Refined.

[Files]
Source: "..\target\release\tracksmith.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "installed.marker"; DestDir: "{app}"; DestName: ".tracksmith-installed"; Flags: ignoreversion
Source: "README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\notices\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}\notices"; Flags: ignoreversion
Source: "..\docs\notices\rust-stdlib\*"; DestDir: "{app}\notices\rust-stdlib"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\docs\notices\XFold-LICENSE.txt"; DestDir: "{app}\notices"; Flags: ignoreversion
Source: "..\data\README.md"; DestDir: "{app}\notices"; DestName: "language-data.md"; Flags: ignoreversion
Source: "..\docs\NAMING.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\REAL-WORLD-TESTING.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\UPDATES.md"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{group}\Tracksmith"; Filename: "{app}\tracksmith.exe"
Name: "{group}\Uninstall Tracksmith"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\tracksmith.exe"; Description: "Open Tracksmith"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
procedure RemoveOwnedExplorerEntry(Suffix: String);
var
  Key, Command, Prefix: String;
begin
  Key := 'Software\Classes\' + Suffix + '\shell\Tracksmith.Open';
  Prefix := '"' + ExpandConstant('{app}\tracksmith.exe') + '" ';
  if RegQueryStringValue(HKCU, Key + '\command', '', Command) then
    if CompareText(Copy(Command, 1, Length(Prefix)), Prefix) = 0 then
      RegDeleteKeyIncludingSubkeys(HKCU, Key);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command, Expected: String;
begin
  if CurUninstallStep = usUninstall then begin
    RemoveOwnedExplorerEntry('SystemFileAssociations\.mkv');
    RemoveOwnedExplorerEntry('SystemFileAssociations\.mka');
    RemoveOwnedExplorerEntry('Directory');
    RemoveOwnedExplorerEntry('Directory\Background');
    Expected := '"' + ExpandConstant('{app}\tracksmith.exe') + '" --background';
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Tracksmith', Command) then
      if CompareText(Command, Expected) = 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Tracksmith');
  end;
end;

