; Remove only this application's current-user startup registration on uninstall.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Nauctions"
  ${EndIf}
!macroend
