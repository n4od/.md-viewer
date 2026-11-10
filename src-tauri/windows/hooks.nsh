; NSIS hooks for MD Viewer.
;
; Tauri's generated installer writes the ProgId, its icon and open command, the
; extension default and a "previous default" backup. It does NOT write the
; "Open with" candidate entries, which is what makes an app appear with a proper
; name instead of a blank row. These hooks add exactly what
; src-tauri/src/main.rs shell_default::register() writes at runtime, so the
; installer, the app and scripts/register-file-association.ps1 agree on one
; ProgId and one set of keys.
;
; Keep the extension list in sync with CLAIM_EXTENSIONS (main.rs) and
; fileAssociations[].ext (tauri.conf.json).

; Removes everything this project adds for one extension. Tauri's own
; APP_UNASSOCIATE deletes the ProgId and restores the extension default, but it
; leaves the "<ProgId>_backup" value and can restore a default that points at the
; ProgId it just deleted. This also runs after that, so it can finalise.
!macro MDVIEWER_CLEAN_EXT EXT PROGID
  DeleteRegValue SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "${PROGID}"
  DeleteRegValue SHCTX "Software\Classes\.${EXT}" "${PROGID}_backup"

  ReadRegStr $0 SHCTX "Software\Classes\.${EXT}" ""
  StrCmp $0 "${PROGID}" 0 +2
    DeleteRegValue SHCTX "Software\Classes\.${EXT}" ""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The key the "Open with" list is enumerated from, with the label it shows.
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "ApplicationName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "ApplicationCompany" "${MANUFACTURER}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "ApplicationIcon" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

  ; SupportedTypes is what the app is advertised as able to open.
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".md" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".markdown" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mdown" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mkd" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mkdn" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mdwn" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mdtxt" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mdtext" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".txt" ""

  ; Candidate handler per extension. Tauri only sets the extension default, so
  ; without this the app is missing from "Choose another app" on a clean machine.
  ; The ProgId comes from bundle.fileAssociations[].name.
  WriteRegStr SHCTX "Software\Classes\.md\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.markdown\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mdown\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mkd\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mkdn\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mdwn\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mdtxt\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.mdtext\OpenWithProgids" "MDViewer.Markdown" ""
  WriteRegStr SHCTX "Software\Classes\.txt\OpenWithProgids" "MDViewer.Text" ""

  ; Make Explorer pick the change up without a sign-out.
  System::Call "shell32::SHChangeNotify(i,i,i,i) (0x08000000, 0x1000, 0, 0)"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Ours, not Tauri's: the Applications key and the candidate entries.
  DeleteRegKey SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe"

  !insertmacro MDVIEWER_CLEAN_EXT md MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT markdown MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mdown MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mkd MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mkdn MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mdwn MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mdtxt MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT mdtext MDViewer.Markdown
  !insertmacro MDVIEWER_CLEAN_EXT txt MDViewer.Text

  ; WebView2's user data (cache and localStorage: theme, recent files). Tauri's
  ; uninstaller leaves it behind. The folder is named after the bundle identifier.
  RMDir /r "$LOCALAPPDATA\${BUNDLEID}"
!macroend