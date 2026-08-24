// SPDX-License-Identifier: GPL-3.0-or-later
import QtCore
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtWebEngine
import io.github.alescdb.mailviewer

ApplicationWindow {
    id: window
    width: 1000
    height: 750
    visible: true
    title: message.subject.length > 0 ? message.subject : qsTr("MailViewer")

    // The range the GTK application clamps to, so both behave the same.
    readonly property real zoomMin: 0.3
    readonly property real zoomMax: 5.0
    readonly property real zoomStep: 0.1

    // Kept between sessions, the way the GTK one keeps it in gsettings.
    Settings {
        id: prefs
        property real zoom: 1.0
    }

    // The view only writes pdfs, so printing goes through one and it is
    // removed as soon as the job is out.
    property bool printing: false

    function printMessage() {
        var folder = StandardPaths.writableLocation(StandardPaths.RuntimeLocation)
        if (folder.toString().length === 0) {
            folder = StandardPaths.writableLocation(StandardPaths.TempLocation)
        }
        window.printing = true
        view.printToPdf(folder.toString().replace(/^file:\/\//, "") + "/mailviewer-kde-print.pdf")
    }

    function clampZoom(value) {
        return Math.min(window.zoomMax, Math.max(window.zoomMin, value))
    }

    function setZoom(value) {
        // Stepping by 0.1 drifts, and the label would show 89% for 0.9.
        prefs.zoom = Math.round(window.clampZoom(value) * 100) / 100
    }

    // sequences, not sequence: zoom in alone answers to three key bindings.
    Shortcut {
        sequences: [StandardKey.ZoomIn, "Ctrl+="]
        onActivated: window.setZoom(prefs.zoom + window.zoomStep)
    }
    Shortcut {
        sequences: [StandardKey.ZoomOut]
        onActivated: window.setZoom(prefs.zoom - window.zoomStep)
    }
    Shortcut {
        sequences: ["Ctrl+0"]
        onActivated: window.setZoom(1.0)
    }
    Shortcut {
        sequences: [StandardKey.Print]
        onActivated: window.printMessage()
    }

    Keyring {
        id: keyring
        Component.onCompleted: keyring.refresh()
    }

    Message {
        id: message
        Component.onCompleted: {
            if (Qt.application.arguments.length > 1) {
                message.open(Qt.application.arguments[1])
            }
        }
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent

            ToolButton {
                text: qsTr("Open")
                onClicked: openDialog.open()
            }
            ToolButton {
                text: qsTr("Find")
                onClicked: {
                    searchBar.visible = !searchBar.visible
                    if (searchBar.visible) searchField.forceActiveFocus()
                    else view.findText("")
                }
            }
            ToolButton {
                text: qsTr("Export as PDF")
                onClicked: pdfDialog.open()
            }
            ToolButton {
                text: qsTr("Keys")
                onClicked: keyWindow.show()
            }
            ToolButton {
                text: qsTr("Print")
                enabled: !window.printing
                onClicked: window.printMessage()
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                text: "\u2212"
                enabled: prefs.zoom > window.zoomMin
                onClicked: window.setZoom(prefs.zoom - window.zoomStep)
            }
            ToolButton {
                text: Math.round(window.clampZoom(prefs.zoom) * 100) + "%"
                onClicked: window.setZoom(1.0)
            }
            ToolButton {
                text: "+"
                enabled: prefs.zoom < window.zoomMax
                onClicked: window.setZoom(prefs.zoom + window.zoomStep)
            }
            CheckBox {
                id: showImages
                text: qsTr("Show remote images")
                checked: false
                // The policy travels with the html, so the message is rendered again.
                onToggled: {
                    message.allow_remote = checked
                    message.reload()
                }
            }
        }
    }

    // Its own keyring, inside the data folder of the application. The keys of
    // the user, in ~/.gnupg, are never read: what goes in here is imported.
    Window {
        id: keyWindow
        title: qsTr("Keys")
        width: 620
        height: 420
        color: palette.window

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8

            Label {
                text: qsTr("Keys imported into MailViewer. Your own keyring is not read.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Frame {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: keyList
                    anchors.fill: parent
                    clip: true
                    model: keyring.keys
                    spacing: 2

                    delegate: RowLayout {
                        id: row
                        required property int index
                        required property string modelData
                        width: keyList.width

                        Label {
                            text: row.modelData
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        ToolButton {
                            text: qsTr("Remove")
                            onClicked: {
                                removeDialog.fingerprint = keyring.fingerprints[row.index]
                                removeDialog.who = row.modelData
                                removeDialog.open()
                            }
                        }
                    }

                    Label {
                        anchors.centerIn: parent
                        visible: keyList.count === 0
                        text: qsTr("No keys yet.")
                    }
                }
            }

            Label {
                text: keyring.error
                visible: text.length > 0
                color: "red"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: keyring.folder
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                    opacity: 0.7
                }
                Button {
                    text: qsTr("Import a key")
                    onClicked: keyDialog.open()
                }
            }
        }
    }

    Dialog {
        id: removeDialog
        property string fingerprint: ""
        property string who: ""
        parent: keyWindow.contentItem
        anchors.centerIn: parent
        modal: true
        title: qsTr("Remove this key?")
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            var error = keyring.remove_key(removeDialog.fingerprint)
            if (error.length > 0) errorLabel.text = error
        }

        Label {
            text: removeDialog.who
            wrapMode: Text.Wrap
        }
    }

    FileDialog {
        id: keyDialog
        title: qsTr("Import a key")
        nameFilters: [qsTr("Key files (*.asc *.gpg *.pgp *.key)"), qsTr("All files (*)")]
        onAccepted: {
            var error = keyring.add_key(selectedFile)
            if (error.length > 0) errorLabel.text = error
        }
    }

    FileDialog {
        id: openDialog
        title: qsTr("Open a message")
        nameFilters: [qsTr("Mail files (*.eml *.msg)"), qsTr("All files (*)")]
        onAccepted: message.open(selectedFile)
    }

    FileDialog {
        id: pdfDialog
        title: qsTr("Export as PDF")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "pdf"
        nameFilters: [qsTr("PDF (*.pdf)")]
        onAccepted: view.printToPdf(selectedFile)
    }

    FileDialog {
        id: saveDialog
        property int index: -1
        title: qsTr("Save the attachment")
        fileMode: FileDialog.SaveFile
        onAccepted: {
            var error = message.save_attachment(index, selectedFile)
            if (error.length > 0) errorLabel.text = error
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 6

        GridLayout {
            columns: 2
            columnSpacing: 10
            Label { text: qsTr("From:"); font.bold: true }
            Label { text: message.from; Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: qsTr("To:"); font.bold: true }
            Label { text: message.to; Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: qsTr("Subject:"); font.bold: true }
            Label { text: message.subject; Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: qsTr("Date:"); font.bold: true }
            Label { text: message.date; Layout.fillWidth: true }
        }

        // Nothing is checked and nothing is decrypted, so this only repeats what
        // the message says about itself.
        Rectangle {
            visible: message.protection.length > 0
            Layout.fillWidth: true
            implicitHeight: protectionLabel.implicitHeight + 12
            color: palette.alternateBase
            border.color: palette.mid
            radius: 4

            Label {
                id: protectionLabel
                anchors.fill: parent
                anchors.margins: 6
                wrapMode: Text.Wrap
                text: message.protection === "encrypted"
                    ? qsTr("This message is encrypted. MailViewer cannot show what is inside.")
                    : qsTr("This message is signed. MailViewer does not check the signature.")
            }
        }

        Label {
            id: errorLabel
            visible: text.length > 0
            text: message.error
            color: "red"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }

        // Rides on the web view, the way the GTK one does.
        Rectangle {
            id: searchBar
            visible: false
            Layout.fillWidth: true
            implicitHeight: searchRow.implicitHeight + 8
            color: "transparent"

            RowLayout {
                id: searchRow
                anchors.fill: parent
                TextField {
                    id: searchField
                    Layout.fillWidth: true
                    placeholderText: qsTr("Find in message")
                    onTextChanged: view.findText(text)
                    onAccepted: view.findText(text)
                    Keys.onEscapePressed: {
                        view.findText("")
                        searchBar.visible = false
                    }
                }
                ToolButton {
                    text: qsTr("Previous")
                    onClicked: view.findText(searchField.text, WebEngineView.FindBackward)
                }
                ToolButton {
                    text: qsTr("Next")
                    onClicked: view.findText(searchField.text)
                }
            }
        }

        // The body arrives sanitized from the core, policy included. The
        // settings here are the same the GTK application uses.
        WebEngineView {
            id: view
            Layout.fillWidth: true
            Layout.fillHeight: true

            zoomFactor: window.clampZoom(prefs.zoom)

            settings.javascriptEnabled: false
            settings.localContentCanAccessFileUrls: false
            settings.localContentCanAccessRemoteUrls: false
            settings.autoLoadImages: showImages.checked

            profile: WebEngineProfile {
                // Nothing a message pulls in has any business surviving on disk.
                offTheRecord: true
            }

            onNavigationRequested: function(request) {
                // Links go to the system handler, and only the schemes a mail
                // is expected to link to.
                var url = request.url.toString()
                if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation) {
                    request.action = WebEngineNavigationRequest.IgnoreRequest
                    if (/^(https?|mailto):/i.test(url)) {
                        Qt.openUrlExternally(request.url)
                    } else {
                        console.warn("refused:", url)
                    }
                }
            }

            onPdfPrintingFinished: function(path, success) {
                if (!window.printing) {
                    if (!success) errorLabel.text = qsTr("Could not write the pdf")
                    return
                }
                window.printing = false
                if (!success) {
                    errorLabel.text = qsTr("Could not prepare the message for printing")
                    return
                }
                var error = message.print_pdf(path)
                if (error.length > 0) errorLabel.text = error
            }

            Component.onCompleted: loadHtml(message.body)

            Connections {
                target: message
                function onBodyChanged() { view.loadHtml(message.body) }
            }
        }

        // Attachments, collapsed until there are any.
        Frame {
            Layout.fillWidth: true
            visible: message.attachments.length > 0
            Layout.maximumHeight: 160

            ColumnLayout {
                anchors.fill: parent
                Label {
                    text: qsTr("%n attachment(s)", "", message.attachments.length)
                    font.bold: true
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: message.attachments
                    delegate: RowLayout {
                        width: ListView.view.width
                        Label {
                            text: modelData
                            Layout.fillWidth: true
                            elide: Text.ElideMiddle
                        }
                        ToolButton {
                            text: qsTr("Open")
                            onClicked: {
                                var uri = message.attachment_to_tmp(index)
                                if (uri.length > 0) Qt.openUrlExternally(uri)
                                else errorLabel.text = qsTr("Could not extract the attachment")
                            }
                        }
                        ToolButton {
                            text: qsTr("Save as")
                            onClicked: {
                                saveDialog.index = index
                                saveDialog.open()
                            }
                        }
                    }
                }
            }
        }
    }
}
