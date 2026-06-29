; openLoom NSIS custom installer script — adds "Register CLI tools to PATH" checkbox
; Everything that needs MUI macros lives inside customPageAfterChangeDir because
; MUI2.nsh is NOT yet loaded when this file is !included; it IS loaded by the
; time assistedInstaller.nsh invokes !insertmacro customPageAfterChangeDir.
;
; customPageAfterChangeDir and customInstall are only expanded when building the
; installer (BUILD_UNINSTALLER undefined). The uninstaller build never expands
; them, so a top-level Var here would be "not referenced" in that build and
; trigger NSIS warning 6001, which -WX turns into a fatal error. Guard the
; installer-only vars and macros with !ifndef BUILD_UNINSTALLER.

!ifndef BUILD_UNINSTALLER
Var CheckboxCLI
Var AddCLIToPath
Var CliTitle
Var CliSubtitle
Var CliCheckboxText

!macro customPageAfterChangeDir
  Function CLIPageCreate
    nsDialogs::Create 1018
    Pop $0

    ; CLI page text by installer language: 2052=SimpChinese, 1028=TradChinese, 1033=English.
    ; LangString cannot be used here: this file is !included before addLangs runs, and
    ; customPageAfterChangeDir also expands before addLangs, so ${LANG_*} is unavailable.
    ${If} $LANGUAGE == 2052
      StrCpy $CliTitle "CLI 工具"
      StrCpy $CliSubtitle "选择是否注册命令行工具。"
      StrCpy $CliCheckboxText "将 loom CLI 添加到系统 PATH$\n允许在命令提示符、PowerShell 等终端使用 'loom'。"
    ${ElseIf} $LANGUAGE == 1028
      StrCpy $CliTitle "CLI 工具"
      StrCpy $CliSubtitle "選擇是否註冊命令列工具。"
      StrCpy $CliCheckboxText "將 loom CLI 加入系統 PATH$\n允許在命令提示字元、PowerShell 等終端機使用 'loom'。"
    ${Else}
      StrCpy $CliTitle "CLI Tools"
      StrCpy $CliSubtitle "Choose whether to register command-line tools."
      StrCpy $CliCheckboxText "Add loom CLI to system PATH$\nAllows using 'loom' from Command Prompt, PowerShell, and other terminals."
    ${EndIf}

    !insertmacro MUI_HEADER_TEXT "$CliTitle" "$CliSubtitle"
    ${NSD_CreateCheckbox} 0 30u 100% 24u "$CliCheckboxText"
    Pop $CheckboxCLI
    ${NSD_SetState} $CheckboxCLI ${BST_CHECKED}

    nsDialogs::Show
  FunctionEnd

  Function CLIPageLeave
    ${NSD_GetState} $CheckboxCLI $AddCLIToPath
  FunctionEnd

  Page Custom CLIPageCreate CLIPageLeave
!macroend

!macro customInstall
  ${If} $AddCLIToPath == ${BST_CHECKED}
    ${If} $hasPerMachineInstallation == "1"
      EnVar::SetHKLM
    ${Else}
      EnVar::SetHKCU
    ${EndIf}
    EnVar::AddValue "PATH" "$INSTDIR\resources\engine"
    Pop $0
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=500
  ${EndIf}
!macroend
!endif

!macro customUnInstall
  ${If} $INSTDIR != ""
    ReadRegStr $0 HKLM "${INSTALL_REGISTRY_KEY}" InstallLocation
    ${If} $0 != ""
      EnVar::SetHKLM
    ${Else}
      EnVar::SetHKCU
    ${EndIf}
    EnVar::DeleteValue "PATH" "$INSTDIR\resources\engine"
    Pop $0
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=500
  ${EndIf}
!macroend
