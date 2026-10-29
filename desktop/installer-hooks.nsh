; Remove only this application's current-user startup registration on uninstall.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Nauctions"
!macroend
