Unicode true
RequestExecutionLevel user

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION is required"
!endif
!ifndef NATIVE_BINARY
  !error "NATIVE_BINARY is required"
!endif
!ifndef NATIVE_HOST_LIBRARY
  !error "NATIVE_HOST_LIBRARY is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
!ifndef APP_ICON
  !error "APP_ICON is required"
!endif

!define PRODUCT_NAME "LiliaCode Native Preview"
!define PRODUCT_EXE "liliacode-native-preview.exe"
!define PRODUCT_HOST_LIBRARY "lilia_native_host.dll"
!define PRODUCT_CLI "liliacode-native.cmd"
!define PRODUCT_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiliaCodeNativePreview"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\LiliaCode Native Preview"
InstallDirRegKey HKCU "${PRODUCT_REG_KEY}" "InstallLocation"
Icon "${APP_ICON}"
UninstallIcon "${APP_ICON}"
BrandingText "LiliaCode Native Preview"
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "启动 LiliaCode Native Preview"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

Var IsUpdate
Var UpdatePid

!macro WRITE_PATH_SCRIPT SCRIPT_PATH MODE
  FileOpen $0 "${SCRIPT_PATH}" w
  FileWrite $0 "$$installDir = @'$\r$\n"
  FileWrite $0 "$INSTDIR$\r$\n"
  FileWrite $0 "'@$\r$\n"
  FileWrite $0 "$$target = [IO.Path]::GetFullPath($$installDir).TrimEnd('\')$\r$\n"
  FileWrite $0 "$$current = [Environment]::GetEnvironmentVariable('Path', 'User')$\r$\n"
  FileWrite $0 "$$parts = @()$\r$\n"
  FileWrite $0 "if ($$current) {$\r$\n"
  FileWrite $0 "  $$parts = $$current -split ';' | Where-Object {$\r$\n"
  FileWrite $0 "    $$_ -and ([IO.Path]::GetFullPath($$_).TrimEnd('\') -ine $$target)$\r$\n"
  FileWrite $0 "  }$\r$\n"
  FileWrite $0 "}$\r$\n"
  !if "${MODE}" == "install"
    FileWrite $0 "$$parts += $$target$\r$\n"
  !endif
  FileWrite $0 "[Environment]::SetEnvironmentVariable('Path', ($$parts -join ';'), 'User')$\r$\n"
  FileClose $0
!macroend

!macro RUN_PATH_SCRIPT MODE
  StrCpy $1 "$TEMP\liliacode-native-path-${MODE}.ps1"
  !insertmacro WRITE_PATH_SCRIPT "$1" "${MODE}"
  ExecWait 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1"'
  Delete "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

Function .onInit
  SetShellVarContext current
  StrCpy $IsUpdate "0"
  StrCpy $UpdatePid ""
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    StrCpy $IsUpdate "1"
    SetSilent silent
  ${EndIf}
  ClearErrors
  ${GetOptions} $CMDLINE "/passive" $0
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATEPID=" $UpdatePid
  ${If} $IsUpdate == "1"
    ${If} $UpdatePid == ""
      Abort "Native Preview update requires the source process id."
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext current
FunctionEnd

Section "Install"
  ${If} $IsUpdate == "1"
    DetailPrint "Waiting for Native Preview process $UpdatePid to exit"
    System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i $UpdatePid) p .r0'
    ${If} $0 != 0
      System::Call 'kernel32::WaitForSingleObject(p r0, i 60000) i .r1'
      System::Call 'kernel32::CloseHandle(p r0)'
      ${If} $1 != 0
        DetailPrint "Native Preview did not exit before the update timeout"
        SetErrorLevel 2
        Quit
      ${EndIf}
    ${EndIf}
  ${EndIf}

  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /oname=${PRODUCT_EXE} "${NATIVE_BINARY}"
  File /oname=${PRODUCT_HOST_LIBRARY} "${NATIVE_HOST_LIBRARY}"

  FileOpen $0 "$INSTDIR\${PRODUCT_CLI}" w
  FileWrite $0 "@echo off$\r$\n"
  FileWrite $0 "$\"%~dp0${PRODUCT_EXE}$\" %*$\r$\n"
  FileClose $0
  !insertmacro RUN_PATH_SCRIPT "install"

  CreateDirectory "$SMPROGRAMS\LiliaCode Native Preview"
  CreateShortcut "$SMPROGRAMS\LiliaCode Native Preview\LiliaCode Native Preview.lnk" "$INSTDIR\${PRODUCT_EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "DisplayIcon" "$INSTDIR\${PRODUCT_EXE}"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "Publisher" "sena-nana"
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "${PRODUCT_REG_KEY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegDWORD HKCU "${PRODUCT_REG_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${PRODUCT_REG_KEY}" "NoRepair" 1

  ${If} $IsUpdate == "1"
    Exec '"$INSTDIR\${PRODUCT_EXE}"'
  ${EndIf}
SectionEnd

Section "Uninstall"
  !insertmacro RUN_PATH_SCRIPT "uninstall"
  Delete "$INSTDIR\${PRODUCT_CLI}"
  Delete "$INSTDIR\${PRODUCT_EXE}"
  Delete "$INSTDIR\${PRODUCT_HOST_LIBRARY}"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\LiliaCode Native Preview\LiliaCode Native Preview.lnk"
  RMDir "$SMPROGRAMS\LiliaCode Native Preview"
  DeleteRegKey HKCU "${PRODUCT_REG_KEY}"
  RMDir "$INSTDIR"
SectionEnd
