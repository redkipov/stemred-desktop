Var StemAutostartCheckbox
Var StemAutostartState
Var StemOptionsTitle
Var StemOptionsSubtitle
Var StemOptionsText
Var StemAutostartText

Page custom StemInstallOptionsPage StemInstallOptionsLeave

Function StemInstallOptionsDefaults
  ${If} $StemAutostartState == ""
    StrCpy $StemAutostartState ${BST_CHECKED}
  ${EndIf}
FunctionEnd

Function StemInstallOptionsTexts
  ${If} $LANGUAGE == 1049
    StrCpy $StemOptionsTitle "Параметры установки"
    StrCpy $StemOptionsSubtitle "Выберите, что включить сразу после установки"
    StrCpy $StemOptionsText "Ярлык на рабочем столе и запуск приложения после установки можно отключить на завершающем шаге."
    StrCpy $StemAutostartText "Запускать stemred вместе с Windows"
  ${Else}
    StrCpy $StemOptionsTitle "Installation options"
    StrCpy $StemOptionsSubtitle "Choose what to enable after installation"
    StrCpy $StemOptionsText "Desktop shortcut and app launch after installation can be disabled on the final step."
    StrCpy $StemAutostartText "Start stemred with Windows"
  ${EndIf}
FunctionEnd

Function StemInstallOptionsPage
  ${If} ${Silent}
    Abort
  ${EndIf}

  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  Call StemInstallOptionsDefaults
  Call StemInstallOptionsTexts
  !insertmacro MUI_HEADER_TEXT "$StemOptionsTitle" "$StemOptionsSubtitle"

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 28u "$StemOptionsText"
  Pop $0

  ${NSD_CreateCheckbox} 0 46u 100% 12u "$StemAutostartText"
  Pop $StemAutostartCheckbox
  SendMessage $StemAutostartCheckbox ${BM_SETCHECK} $StemAutostartState 0

  nsDialogs::Show
FunctionEnd

Function StemInstallOptionsLeave
  ${NSD_GetState} $StemAutostartCheckbox $StemAutostartState
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${If} ${Errors}
    Call StemInstallOptionsDefaults
    ${If} $StemAutostartState == ${BST_CHECKED}
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "StemRed"
      WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "stemred" "$\"$INSTDIR\stem_desktop.exe$\""
    ${Else}
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "StemRed"
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "stemred"
    ${EndIf}
    WriteRegDWORD HKCU "Software\STEM\Messenger\Desktop" "AutostartInitialized" 1
  ${EndIf}
!macroend
