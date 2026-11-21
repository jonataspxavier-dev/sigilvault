; Ganchos do instalador do SigilVault (NSIS, chamados pelo Tauri).
; SigilVault installer hooks (NSIS, called by Tauri).
;
; Registra o conector da extensão (Native Messaging) para todos os usuários. Os arquivos .json
; ficam na pasta protegida da instalação e apontam para sigilvault-host.exe por caminho relativo.
; Registers the extension connector (Native Messaging) for all users. The .json files live in the
; protected install folder and point to sigilvault-host.exe by relative path.

!macro NSIS_HOOK_POSTINSTALL
  SetRegView 64
  ; Chrome e Opera / Chrome and Opera
  WriteRegStr HKLM "Software\Google\Chrome\NativeMessagingHosts\app.sigilvault.host" "" "$INSTDIR\app.sigilvault.host.chromium.json"
  ; Brave
  WriteRegStr HKLM "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\app.sigilvault.host" "" "$INSTDIR\app.sigilvault.host.chromium.json"
  ; Firefox
  WriteRegStr HKLM "Software\Mozilla\NativeMessagingHosts\app.sigilvault.host" "" "$INSTDIR\app.sigilvault.host.firefox.json"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  SetRegView 64
  DeleteRegKey HKLM "Software\Google\Chrome\NativeMessagingHosts\app.sigilvault.host"
  DeleteRegKey HKLM "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\app.sigilvault.host"
  DeleteRegKey HKLM "Software\Mozilla\NativeMessagingHosts\app.sigilvault.host"
  ; "Iniciar com o Windows" (do usuário que desinstala). / "Start with Windows" (of the uninstalling user).
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "SigilVault"
!macroend
