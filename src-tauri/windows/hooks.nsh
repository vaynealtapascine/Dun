; Dun writes three things outside its own folder, all under HKCU, and the
; installer knows nothing about them:
;
;   - the AppUserModelId that gives its toasts a name and icon,
;   - the CLSID that lets Windows start Dun when a toast button is pressed,
;   - the Run entry that starts it at login.
;
; Left behind, the CLSID would point at a deleted exe and the Run entry would
; fail at every login. Uninstalling should leave nothing running or registered,
; so clear all three. Ids must match `desktop::toast::Identity::RELEASE` and the
; product name the autostart plugin registers under.
;
; (The firewall rule for sync is not removed: it was added with elevation, and
; the uninstaller runs as the current user. It allows an exe that no longer
; exists, so it lets nothing through.)

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\app.dun"
  DeleteRegKey HKCU "Software\Classes\CLSID\{5E207284-DEBB-4DA4-91F5-F05AD60E7F18}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Dun"
!macroend
