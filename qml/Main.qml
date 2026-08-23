// SPDX-License-Identifier: GPL-3.0-or-later
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
            Item { Layout.fillWidth: true }
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
                if (!success) errorLabel.text = qsTr("Could not write the pdf")
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
