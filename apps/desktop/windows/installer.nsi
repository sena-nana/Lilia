Unicode true
RequestExecutionLevel user

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION is required"
!endif
!ifndef DESKTOP_BINARY
  !error "DESKTOP_BINARY is required"
!endif
!ifndef HOST_LIBRARY
  !error "HOST_LIBRARY is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
!ifndef APP_ICON
  !error "APP_ICON is required"
!endif

!define PRODUCT_NAME "LiliaCode"
!define PRODUCT_EXE "liliacode.exe"
!define PRODUCT_HOST_LIBRARY "liliacode_host.dll"
!define PRODUCT_CLI "liliacode.cmd"
!define PRODUCT_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiliaCode"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\LiliaCode"
InstallDirRegKey HKCU "${PRODUCT_REG_KEY}" "InstallLocation"
Icon "${APP_ICON}"
UninstallIcon "${APP_ICON}"
BrandingText "LiliaCode"
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "启动 LiliaCode"

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
  StrCpy $1 "$TEMP\liliacode-path-${MODE}.ps1"
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
      Abort "LiliaCode update requires the source process id."
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext current
FunctionEnd

Section "Install"
  ${If} $IsUpdate == "1"
    DetailPrint "Waiting for LiliaCode process $UpdatePid to exit"
    System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i $UpdatePid) p .r0'
    ${If} $0 != 0
      System::Call 'kernel32::WaitForSingleObject(p r0, i 60000) i .r1'
      System::Call 'kernel32::CloseHandle(p r0)'
      ${If} $1 != 0
        DetailPrint "LiliaCode did not exit before the update timeout"
        SetErrorLevel 2
        Quit
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ${If} $IsUpdate != "1"
    IfFileExists "$INSTDIR\lilia.exe" 0 legacy_desktop_done
    IfFileExists "$INSTDIR\uninstall.exe" 0 legacy_desktop_cleanup
    DetailPrint "Removing the legacy desktop host while preserving LILIA_HOME"
    ExecWait '"$INSTDIR\uninstall.exe" /S'
    Goto legacy_desktop_done
legacy_desktop_cleanup:
    Delete "$INSTDIR\lilia.exe"
legacy_desktop_done:
  ${EndIf}

  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /oname=${PRODUCT_EXE} "${DESKTOP_BINARY}"
  File /oname=${PRODUCT_HOST_LIBRARY} "${HOST_LIBRARY}"

  FileOpen $0 "$INSTDIR\${PRODUCT_CLI}" w
  FileWrite $0 "@echo off$\r$\n"
  FileWrite $0 "$\"%~dp0${PRODUCT_EXE}$\" %*$\r$\n"
  FileClose $0
  !insertmacro RUN_PATH_SCRIPT "install"

  CreateDirectory "$SMPROGRAMS\LiliaCode"
  CreateShortcut "$SMPROGRAMS\LiliaCode\LiliaCode.lnk" "$INSTDIR\${PRODUCT_EXE}"
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
  Delete "$SMPROGRAMS\LiliaCode\LiliaCode.lnk"
  RMDir "$SMPROGRAMS\LiliaCode"
  DeleteRegKey HKCU "${PRODUCT_REG_KEY}"
  RMDir "$INSTDIR"
SectionEnd
