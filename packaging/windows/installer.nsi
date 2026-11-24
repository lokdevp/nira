; NSIS installer for nira (Windows).
; Run from the crate root: makensis -DVERSION=x.y.z packaging/windows/installer.nsi

!ifndef VERSION
  !define VERSION "0.1.0"
!endif

!define APPNAME "Nira"
!define BIN "nira.exe"
!ifndef SOURCE_DIR
  !define SOURCE_DIR "dist\nira-win64"
!endif
!ifndef OUTFILE
  !define OUTFILE "dist\nira-setup-${VERSION}.exe"
!endif
!define REGKEY "Software\${APPNAME}"
!define CLASSES "Software\Classes"
; Comic book archives, which Nira opens by default unless another app owns them.
!define PROGID_COMIC "Nira.ComicBook"
; Everything else Nira reads, offered under "Open with" only.
!define PROGID_FILE "Nira.File"
!define CAPS "${REGKEY}\Capabilities"

!include "LogicLib.nsh"

Name "${APPNAME} ${VERSION}"
Icon "${SOURCE_DIR}\nira.ico"
UninstallIcon "${SOURCE_DIR}\nira.ico"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APPNAME}"
InstallDirRegKey HKLM "${REGKEY}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

; A ProgID whose open verb runs nira.exe on the file.
!macro ProgId ID DESC
    WriteRegStr HKLM "${CLASSES}\${ID}" "" "${DESC}"
    WriteRegStr HKLM "${CLASSES}\${ID}" "FriendlyTypeName" "${DESC}"
    WriteRegStr HKLM "${CLASSES}\${ID}\DefaultIcon" "" "$INSTDIR\nira.ico"
    WriteRegStr HKLM "${CLASSES}\${ID}\shell\open\command" "" '"$INSTDIR\${BIN}" "%1"'
!macroend

; A comic extension: listed under Open with, and the default handler only
; when no other app has claimed it. OwnedExt records which defaults we set,
; so the uninstaller clears those and nothing else.
!macro AssocComic EXT
    WriteRegStr HKLM "${CLASSES}\.${EXT}\OpenWithProgids" "${PROGID_COMIC}" ""
    WriteRegStr HKLM "${CLASSES}\Applications\${BIN}\SupportedTypes" ".${EXT}" ""
    WriteRegStr HKLM "${CAPS}\FileAssociations" ".${EXT}" "${PROGID_COMIC}"
    ReadRegStr $0 HKLM "${CLASSES}\.${EXT}" ""
    ${If} $0 == ""
    ${OrIf} $0 == "${PROGID_COMIC}"
        WriteRegStr HKLM "${CLASSES}\.${EXT}" "" "${PROGID_COMIC}"
        WriteRegStr HKLM "${REGKEY}\OwnedExt" "${EXT}" "1"
    ${EndIf}
!macroend

; Another format Nira reads: offered under Open with, never made the default.
!macro AssocOther EXT
    WriteRegStr HKLM "${CLASSES}\.${EXT}\OpenWithProgids" "${PROGID_FILE}" ""
    WriteRegStr HKLM "${CLASSES}\Applications\${BIN}\SupportedTypes" ".${EXT}" ""
    WriteRegStr HKLM "${CAPS}\FileAssociations" ".${EXT}" "${PROGID_FILE}"
!macroend

!macro UnassocExt EXT PROGID
    DeleteRegValue HKLM "${CLASSES}\.${EXT}\OpenWithProgids" "${PROGID}"
    DeleteRegKey /ifempty HKLM "${CLASSES}\.${EXT}\OpenWithProgids"
    ReadRegStr $0 HKLM "${REGKEY}\OwnedExt" "${EXT}"
    ReadRegStr $1 HKLM "${CLASSES}\.${EXT}" ""
    ${If} $0 == "1"
    ${AndIf} $1 == "${PROGID}"
        DeleteRegValue HKLM "${CLASSES}\.${EXT}" ""
    ${EndIf}
    DeleteRegKey /ifempty HKLM "${CLASSES}\.${EXT}"
!macroend

!macro ForComic M
    !insertmacro ${M} cbz
    !insertmacro ${M} cbr
    !insertmacro ${M} cbt
    !insertmacro ${M} cb7
!macroend

!macro ForOther M
    !insertmacro ${M} pdf
    !insertmacro ${M} zip
    !insertmacro ${M} rar
    !insertmacro ${M} 7z
    !insertmacro ${M} tar
    !insertmacro ${M} lha
    !insertmacro ${M} lzh
    !insertmacro ${M} jpg
    !insertmacro ${M} jpeg
    !insertmacro ${M} png
    !insertmacro ${M} gif
    !insertmacro ${M} webp
    !insertmacro ${M} bmp
    !insertmacro ${M} tif
    !insertmacro ${M} tiff
!macroend

!macro UnassocComic EXT
    !insertmacro UnassocExt ${EXT} "${PROGID_COMIC}"
!macroend
!macro UnassocOther EXT
    !insertmacro UnassocExt ${EXT} "${PROGID_FILE}"
!macroend

; Tell Explorer the associations changed (SHCNE_ASSOCCHANGED).
!macro NotifyAssocChanged
    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
    SetOutPath "$INSTDIR"
    File /r "${SOURCE_DIR}\*.*"

    ; Start menu shortcut
    CreateDirectory "$SMPROGRAMS\${APPNAME}"
    CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\${BIN}" "" "$INSTDIR\nira.ico"
    CreateShortcut "$SMPROGRAMS\${APPNAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"

    ; Uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"
    WriteRegStr HKLM "${REGKEY}" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
        "DisplayName" "${APPNAME}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
        "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
        "DisplayIcon" "$INSTDIR\nira.ico"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
        "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}" \
        "Publisher" "lokesh"

    ; File associations
    !insertmacro ProgId "${PROGID_COMIC}" "Comic Book Archive"
    !insertmacro ProgId "${PROGID_FILE}" "${APPNAME} Document"
    WriteRegStr HKLM "${CLASSES}\Applications\${BIN}" "FriendlyAppName" "${APPNAME}"
    WriteRegStr HKLM "${CLASSES}\Applications\${BIN}\DefaultIcon" "" "$INSTDIR\nira.ico"
    WriteRegStr HKLM "${CLASSES}\Applications\${BIN}\shell\open\command" "" '"$INSTDIR\${BIN}" "%1"'
    WriteRegStr HKLM "${CAPS}" "ApplicationName" "${APPNAME}"
    WriteRegStr HKLM "${CAPS}" "ApplicationDescription" "Comic book and manga viewer"
    WriteRegStr HKLM "${CAPS}" "ApplicationIcon" "$INSTDIR\nira.ico"
    WriteRegStr HKLM "Software\RegisteredApplications" "${APPNAME}" "${CAPS}"
    !insertmacro ForComic AssocComic
    !insertmacro ForOther AssocOther
    !insertmacro NotifyAssocChanged
SectionEnd

Section "Uninstall"
    ; File associations, before REGKEY (which holds OwnedExt) goes.
    !insertmacro ForComic UnassocComic
    !insertmacro ForOther UnassocOther
    DeleteRegKey HKLM "${CLASSES}\${PROGID_COMIC}"
    DeleteRegKey HKLM "${CLASSES}\${PROGID_FILE}"
    DeleteRegKey HKLM "${CLASSES}\Applications\${BIN}"
    DeleteRegValue HKLM "Software\RegisteredApplications" "${APPNAME}"

    Delete "$INSTDIR\uninstall.exe"
    RMDir /r "$INSTDIR"
    Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
    Delete "$SMPROGRAMS\${APPNAME}\Uninstall.lnk"
    RMDir "$SMPROGRAMS\${APPNAME}"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
    DeleteRegKey HKLM "${REGKEY}"
    !insertmacro NotifyAssocChanged
SectionEnd
