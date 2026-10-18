import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

ApplicationWindow {
    id: window

    required property var backend
    width: 620
    height: 430
    minimumWidth: 520
    minimumHeight: 400
    visible: true
    title: "Gamma Panel"

    property string selectedId: ""
    property string curve: "classic"
    property real brightness: 1
    property real contrast: 1
    property real gamma: 1
    property var monitorOptions: [{ id: "", name: "All monitors" }]
    property string statusText: ""
    property bool statusIsError: false
    property bool mixedMonitorSettings: false

    function neutral(curveName) {
        return curveName === "midgray"
                ? { brightness: 0, contrast: 1, gamma: 1 }
                : { brightness: 1, contrast: 1, gamma: 1 }
    }

    function outputFor(id) {
        for (let i = 0; i < backend.outputs.length; ++i) {
            if (backend.outputs[i].id === id)
                return backend.outputs[i]
        }
        return null
    }

    function loadSelection() {
        let selected = outputFor(selectedId)
        if (selected) {
            curve = selected.curve
            brightness = selected.brightness
            contrast = selected.contrast
            gamma = selected.gamma
            mixedMonitorSettings = false
            return
        }

        if (backend.outputs.length > 0) {
            let first = backend.outputs[0]
            let same = true
            for (let i = 1; i < backend.outputs.length; ++i) {
                let output = backend.outputs[i]
                if (output.curve !== first.curve
                        || output.brightness !== first.brightness
                        || output.contrast !== first.contrast
                        || output.gamma !== first.gamma) {
                    same = false
                    break
                }
            }
            if (same) {
                curve = first.curve
                brightness = first.brightness
                contrast = first.contrast
                gamma = first.gamma
                mixedMonitorSettings = false
                return
            }
        }

        curve = "classic"
        let defaults = neutral(curve)
        brightness = defaults.brightness
        contrast = defaults.contrast
        gamma = defaults.gamma
        mixedMonitorSettings = backend.outputs.length > 1
    }

    function updateMonitorOptions() {
        let options = [{ id: "", name: "All monitors" }]
        for (let i = 0; i < backend.outputs.length; ++i) {
            let output = backend.outputs[i]
            options.push({
                id: output.id,
                name: output.model.length > 0
                        ? output.name + " (" + output.model + ")"
                        : output.name
            })
        }
        monitorOptions = options

        let found = false
        for (let i = 0; i < options.length; ++i) {
            if (options[i].id === selectedId) {
                monitorBox.currentIndex = i
                found = true
                break
            }
        }
        if (!found) {
            selectedId = ""
            monitorBox.currentIndex = 0
        }
        loadSelection()
    }

    function selectMonitor(id) {
        selectedId = id
        statusText = ""
        statusIsError = false
        loadSelection()
    }

    function setCurve(curveName) {
        if (curve === curveName)
            return
        curve = curveName
        let defaults = neutral(curveName)
        brightness = defaults.brightness
        contrast = defaults.contrast
        gamma = defaults.gamma
        statusText = ""
        statusIsError = false
    }

    function setAdjustment(parameter, value) {
        if (parameter === "brightness")
            brightness = value
        else if (parameter === "contrast")
            contrast = value
        else
            gamma = value
        statusText = ""
        statusIsError = false
    }

    function range(parameter) {
        if (curve === "midgray") {
            if (parameter === "brightness") return [-1, 1]
            if (parameter === "contrast") return [0.1, 100]
            return [0.3, 4.4]
        }

        let value = parameter === "brightness" ? brightness
                  : parameter === "contrast" ? contrast : gamma
        if (parameter === "brightness")
            return [Math.min(-5, value), Math.max(5, value)]
        if (parameter === "contrast")
            return [Math.min(-5, value), Math.max(5, value)]
        return [Math.min(0.01, value), Math.max(10, value)]
    }

    function canReset() {
        if (selectedId.length > 0) {
            let output = outputFor(selectedId)
            return output !== null && output.adjusted
        }
        for (let i = 0; i < backend.outputs.length; ++i) {
            if (backend.outputs[i].adjusted)
                return true
        }
        return false
    }

    function applyChanges() {
        statusText = ""
        statusIsError = false
        backend.apply(selectedId, curve, brightness, contrast, gamma)
    }

    function resetChanges() {
        statusText = ""
        statusIsError = false
        backend.reset(selectedId)
    }

    Component.onCompleted: {
        updateMonitorOptions()
    }

    Connections {
        target: window.backend

        function onOutputsChanged() {
            window.updateMonitorOptions()
        }

        function onMessageChanged() {
            if (window.backend.message.length > 0) {
                window.statusText = window.backend.message
                window.statusIsError = window.backend.messageIsError
            }
        }

        function onOperationFinished(success, message) {
            window.statusText = message
            window.statusIsError = !success
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 22
        enabled: !window.backend.busy

        RowLayout {
            Layout.fillWidth: true
            spacing: 18

            ComboBox {
                id: monitorBox
                Layout.fillWidth: true
                model: window.monitorOptions
                textRole: "name"
                valueRole: "id"
                onActivated: window.selectMonitor(currentValue)
            }

            RowLayout {
                spacing: 8

                Label {
                    text: curveSwitch.checked ? "Gamma Panel" : "Classic"
                }

                Switch {
                    id: curveSwitch
                    checked: window.curve === "midgray"
                    onClicked: window.setCurve(checked ? "midgray" : "classic")
                    Accessible.name: "Use Gamma Panel formula"
                }
            }
        }

        Label {
            Layout.fillWidth: true
            visible: window.mixedMonitorSettings
            text: "Monitors have different settings. These values will be applied to all of them."
            wrapMode: Text.WordWrap
            color: palette.placeholderText
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 8
            spacing: 20

            component AdjustmentRow: RowLayout {
                id: row
                required property string title
                required property string parameter
                required property real adjustment
                required property real lower
                required property real upper
                required property var changeValue

                Layout.fillWidth: true
                spacing: 14

                Label {
                    Layout.preferredWidth: 96
                    text: parent.title
                }

                Slider {
                    Layout.fillWidth: true
                    from: parent.lower
                    to: parent.upper
                    value: parent.adjustment
                    stepSize: 0.01
                    onMoved: row.changeValue(value)
                }

                SpinBox {
                    id: spin
                    Layout.preferredWidth: 94
                    from: Math.round(parent.lower * 100)
                    to: Math.round(parent.upper * 100)
                    value: Math.round(parent.adjustment * 100)
                    stepSize: 1
                    editable: true
                    validator: DoubleValidator {
                        bottom: spin.from / 100
                        top: spin.to / 100
                        decimals: 2
                        notation: DoubleValidator.StandardNotation
                    }
                    textFromValue: function(value, locale) {
                        return (value / 100).toLocaleString(locale, "f", 2)
                    }
                    valueFromText: function(text, locale) {
                        return Math.round(Number.fromLocaleString(locale, text) * 100)
                    }
                    onValueModified: row.changeValue(value / 100)
                }
            }

            AdjustmentRow {
                title: "Brightness"
                parameter: "brightness"
                adjustment: window.brightness
                lower: window.range(parameter)[0]
                upper: window.range(parameter)[1]
                changeValue: function(value) { window.setAdjustment(parameter, value) }
            }

            AdjustmentRow {
                title: "Contrast"
                parameter: "contrast"
                adjustment: window.contrast
                lower: window.range(parameter)[0]
                upper: window.range(parameter)[1]
                changeValue: function(value) { window.setAdjustment(parameter, value) }
            }

            AdjustmentRow {
                title: "Gamma"
                parameter: "gamma"
                adjustment: window.gamma
                lower: window.range(parameter)[0]
                upper: window.range(parameter)[1]
                changeValue: function(value) { window.setAdjustment(parameter, value) }
            }
        }

        Label {
            Layout.fillWidth: true
            visible: window.statusText.length > 0
            text: window.statusText
            color: window.statusIsError ? palette.brightText : palette.highlight
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Button {
                text: "Reset"
                enabled: !window.backend.busy && window.canReset()
                onClicked: window.resetChanges()
            }

            Item { Layout.fillWidth: true }

            Button {
                text: window.backend.busy ? "Working…" : "Apply"
                highlighted: true
                enabled: !window.backend.busy && window.backend.outputs.length > 0
                onClicked: window.applyChanges()
            }
        }
    }
}
