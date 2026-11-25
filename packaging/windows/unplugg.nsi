; Windows installer:  makensis -DAppVersion=2.0.0 -DArch=x64 packaging\windows\unplugg.nsi
; Wraps target\<arch>-pc-windows-msvc\release\unplugg.exe into dist\unplugg-windows-<arch>-setup.exe.
; Per-user, so no admin prompt, and the in-app updater can rerun it silently:
; /S waits for the running copy to quit, replaces it, and starts the new one.

!ifndef AppVersion
  !error "Pass -DAppVersion=<version>"
!endif
!if "${Arch}" == "x64"
  !define Target "x86_64-pc-windows-msvc"
!else if "${Arch}" == "arm64"
  !define Target "aarch64-pc-windows-msvc"
!else
  !error "Pass -DArch=x64 or -DArch=arm64"
!endif
!define Uninstall "Software\Microsoft\Windows\CurrentVersion\Uninstall\unplugg"
; the Inno Setup installer of 1.x, replaced on upgrade
!define InnoUninstall "Software\Microsoft\Windows\CurrentVersion\Uninstall\{51117B94-65BE-4934-85A2-E302EEAC19A0}_is1"

!include MUI2.nsh
!include FileFunc.nsh
!include LogicLib.nsh
!include WinVer.nsh
!include x64.nsh

Unicode true
Name "unplugg"
OutFile "..\..\dist\unplugg-windows-${Arch}-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\unplugg"
InstallDirRegKey HKCU "Software\unplugg" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ICON "..\..\assets\logo.ico"
!define MUI_UNICON "..\..\assets\logo.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\unplugg.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open unplugg"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "unplugg needs Windows 10 or later." /SD IDOK
    Abort
  ${EndIf}
  ; x64 also runs on arm64 Windows, arm64 only there
  !if "${Arch}" == "arm64"
    ${IfNot} ${IsNativeARM64}
      MessageBox MB_OK|MB_ICONSTOP "This installer is for ARM64 Windows." /SD IDOK
      Abort
    ${EndIf}
  !else
    ${IfNot} ${IsNativeAMD64}
    ${AndIfNot} ${IsNativeARM64}
      MessageBox MB_OK|MB_ICONSTOP "This installer is for 64-bit Windows." /SD IDOK
      Abort
    ${EndIf}
  !endif
  ; the 1.x updater runs the new installer with Inno's flags
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/VERYSILENT" $R1
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
FunctionEnd

; a silent update starts the new copy too, like the finish page does
Function .onInstSuccess
  ${If} ${Silent}
    Exec '"$INSTDIR\unplugg.exe"'
  ${EndIf}
FunctionEnd

Section "Install"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  ; wait up to 30 s for the running copy to quit, as the updater asks it to
  StrCpy $0 0
  ${DoUntil} $0 >= 60
    ClearErrors
    ${IfNot} ${FileExists} "$INSTDIR\unplugg.exe"
      ${Break}
    ${EndIf}
    FileOpen $1 "$INSTDIR\unplugg.exe" a
    ${IfNot} ${Errors}
      FileClose $1
      ${Break}
    ${EndIf}
    Sleep 500
    IntOp $0 $0 + 1
  ${Loop}
  ${If} $0 >= 60
    MessageBox MB_OK|MB_ICONSTOP "Please quit unplugg and try again." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}

  ; 1.x left its Python and Qt here, ~150 MB
  ${If} ${FileExists} "$INSTDIR\unins000.dat"
    RMDir /r "$INSTDIR\_internal"
    Delete "$INSTDIR\unins000.dat"
    DeleteRegKey HKCU "${InnoUninstall}"
  ${EndIf}

  File "..\..\target\${Target}\release\unplugg.exe"
  WriteUninstaller "$INSTDIR\unins000.exe"
  CreateShortcut "$SMPROGRAMS\unplugg.lnk" "$INSTDIR\unplugg.exe"
  WriteRegStr HKCU "Software\unplugg" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${Uninstall}" "DisplayName" "unplugg"
  WriteRegStr HKCU "${Uninstall}" "DisplayVersion" "${AppVersion}"
  WriteRegStr HKCU "${Uninstall}" "Publisher" "Upa Das"
  WriteRegStr HKCU "${Uninstall}" "URLInfoAbout" "https://github.com/upamanyudas/unplugg"
  WriteRegStr HKCU "${Uninstall}" "DisplayIcon" "$INSTDIR\unplugg.exe"
  WriteRegStr HKCU "${Uninstall}" "UninstallString" '"$INSTDIR\unins000.exe"'
  WriteRegStr HKCU "${Uninstall}" "QuietUninstallString" '"$INSTDIR\unins000.exe" /S'
  WriteRegDWORD HKCU "${Uninstall}" "NoModify" 1
  WriteRegDWORD HKCU "${Uninstall}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\unplugg.lnk"
  Delete "$INSTDIR\unplugg.exe"
  Delete "$INSTDIR\unins000.exe"
  RMDir "$INSTDIR"
  ; unplugg writes its own launch-at-login value; only remove it on uninstall
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "unplugg"
  DeleteRegKey HKCU "${Uninstall}"
  DeleteRegKey HKCU "Software\unplugg"
SectionEnd
