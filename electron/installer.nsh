# installer.nsh — electron-builder NSIS hook
# Runs during install/uninstall to register CLI in system PATH

!macro customInstall
  ; Add the engine directory to user PATH so `loom-server` is available in terminal
  ${AddToPath} "$INSTDIR\resources\engine"
!macroend

!macro customUnInstall
  ; Remove from PATH on uninstall
  ${RemoveFromPath} "$INSTDIR\resources\engine"
!macroend
