// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtWebEngine
import io.github.alescdb.mailviewer

ApplicationWindow {
    id: window
    width: 900
    height: 700
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

        Label {
            visible: message.error.length > 0
            text: message.error
            color: "red"
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

            Component.onCompleted: loadHtml(message.body)

            Connections {
                target: message
                function onBodyChanged() { view.loadHtml(message.body) }
            }
        }
    }
}
