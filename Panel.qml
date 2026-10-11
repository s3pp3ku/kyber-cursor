// qmllint disable missing-property
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Ui

// The Kyber Cursor popup: pick the glow color with a hue / saturation /
// brightness bar, choose the sound, set the volume, switch the drag trail and
// the cursor itself on or off. Every change goes through the `kyber` command
// (the bar widget's act()), so it matches the keybinds and the terminal.
Panel {
  id: root
  moduleName: "io.github.s3pp3ku.kyber-cursor"
  ipcTarget: "io.github.s3pp3ku.kyber-cursor"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root

  readonly property color fg: bar ? bar.foreground : Color.foreground
  readonly property string fontFam: bar ? bar.fontFamily : Style.font.family
  readonly property var cfg: hostWidget ? hostWidget.cfg : ({ active: false, color: "theme", sound: "all", trail: "on", volume: 55 })
  readonly property bool customColor: /^#[0-9a-fA-F]{6}$/.test(String(cfg.color))
  // where the glow color comes from: theme | custom | random
  readonly property string mode: cfg.mode ? String(cfg.mode) : (customColor ? "custom" : "theme")

  // the colour being picked, as hue / saturation / brightness (all 0..1)
  property real hue: 0.75
  property real sat: 0.60
  property real val: 0.95
  readonly property string pickedHex: hsvToHex(hue, sat, val)
  // the color actually in use (or about to be), for the swatch, icon and switches
  readonly property string shownHex: mode === "custom" ? pickedHex
    : (mode === "random" && /^#[0-9a-fA-F]{6}$/.test(String(cfg.random)) ? String(cfg.random) : colorToHex(Color.accent))

  readonly property var presets: ["#ff4d5a", "#ff7a00", "#f5c518", "#3cbf5c", "#26bbd9", "#5a82c8", "#b87bfd", "#ff6ec7"]

  function colorToHex(c) {
    function hx(x) { var n = Math.round(x * 255).toString(16); return n.length < 2 ? "0" + n : n }
    var q = Qt.color(c)
    return "#" + hx(q.r) + hx(q.g) + hx(q.b)
  }

  function hsvToHex(h, s, v) {
    var i = Math.floor(h * 6) % 6, f = h * 6 - Math.floor(h * 6)
    var p = v * (1 - s), q = v * (1 - f * s), t = v * (1 - (1 - f) * s)
    var rgb = [[v, t, p], [q, v, p], [p, v, t], [p, q, v], [t, p, v], [v, p, q]][i]
    function hx(x) { var n = Math.round(x * 255).toString(16); return n.length < 2 ? "0" + n : n }
    return "#" + hx(rgb[0]) + hx(rgb[1]) + hx(rgb[2])
  }

  function setFromHex(hex) {
    var r = parseInt(hex.substr(1, 2), 16) / 255, g = parseInt(hex.substr(3, 2), 16) / 255, b = parseInt(hex.substr(5, 2), 16) / 255
    var mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn, h = 0
    if (d > 0) {
      if (mx === r) h = ((g - b) / d) % 6
      else if (mx === g) h = (b - r) / d + 2
      else h = (r - g) / d + 4
      h /= 6
      if (h < 0) h += 1
    }
    root.hue = h
    root.sat = mx === 0 ? 0 : d / mx
    root.val = mx
  }

  function syncFromConfig() {
    var saved = /^#[0-9a-fA-F]{6}$/.test(String(cfg.custom)) ? String(cfg.custom) : colorToHex(Color.accent)
    setFromHex(saved)
  }
  // follow changes made elsewhere (the terminal, a keybind), but never while a drag is settling
  onCfgChanged: if (!applyTimer.running) syncFromConfig()

  // Rebuilding the cursor takes about a second, so apply when the user pauses
  // instead of on every pixel of a drag.
  Timer {
    id: applyTimer
    interval: 450
    onTriggered: if (root.hostWidget) root.hostWidget.act("color " + Util.shellQuote(root.pickedHex))
  }
  function picked() { applyTimer.restart() }

  function open() { syncFromConfig(); root.controller.show() }
  function close() { root.controller.hide() }
  function toggle() { if (root.opened) root.close(); else root.open() }

  function act(args) { if (hostWidget) hostWidget.act(args) }

  // ---------------------------------------------------------- components
  component ColorBar: Item {
    id: cb
    property var colorAt: function(t) { return "#000000" }
    property real value: 0
    signal moved(real v)
    height: Style.space(24)
    Row {
      anchors.fill: parent
      Repeater {
        model: 56
        Rectangle { width: cb.width / 56; height: cb.height; color: cb.colorAt(index / 55) }
      }
    }
    Rectangle {
      width: Style.space(8); height: cb.height + Style.space(6); y: -Style.space(3)
      x: cb.value * (cb.width - width)
      color: "transparent"; radius: Style.space(3)
      border.width: 2; border.color: "#ffffff"
      Rectangle { anchors.fill: parent; anchors.margins: -1; color: "transparent"; radius: parent.radius + 1; border.width: 1; border.color: "#000000"; opacity: 0.6 }
    }
    MouseArea {
      anchors.fill: parent
      function pick(mx) { cb.moved(Math.max(0, Math.min(1, mx / cb.width))) }
      onPressed: function(m) { pick(m.x) }
      onPositionChanged: function(m) { if (pressed) pick(m.x) }
    }
  }

  component Label: Text {
    color: root.fg
    font.family: root.fontFam
    font.pixelSize: Style.font.body
    textFormat: Text.PlainText
  }

  // ---------------------------------------------------------- panel body
  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: false
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(360))
    contentHeight: panel.fittedContentHeight(contentCol.implicitHeight, Style.space(640))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Column {
        id: contentCol
        anchors.left: parent.left
        anchors.right: parent.right
        spacing: Style.spacing.xl

        PanelHero {
          id: hero
          width: parent.width
          title: "Kyber Cursor"
          meta: root.cfg.active === true ? "ON" : "OFF"
          detail: root.mode === "theme" ? "THEME" : (root.mode === "random" ? "RANDOM" : "CUSTOM")
          foreground: root.fg
          fontFamily: root.fontFam
          iconComponent: Component {
            Text { text: ""; color: root.pickedHex; font.family: root.fontFam; font.pixelSize: Style.font.display }
          }
          trailingControl: Component {
            PanelActionButton {
              iconText: "󰅖"
              tooltipText: "Close (Esc)"
              foreground: hero.foreground
              fontFamily: hero.fontFamily
              onClicked: root.close()
            }
          }
        }

        PanelSeparator { foreground: root.fg }

        PanelSectionHeader { text: "GLOW COLOR"; foreground: root.fg; fontFamily: root.fontFam; font.letterSpacing: 1 }

        // three sources for the glow color
        Row {
          spacing: Style.spacing.sm
          Repeater {
            model: [{ k: "theme", t: "Theme" }, { k: "custom", t: "Custom" }, { k: "random", t: "Random" }]
            Rectangle {
              width: Style.space(96); height: Style.space(30); radius: Style.space(6)
              color: root.mode === modelData.k ? root.shownHex : "transparent"
              border.width: 1; border.color: root.fg
              Label { anchors.centerIn: parent; text: modelData.t
                color: root.mode === modelData.k ? "#000000" : root.fg }
              MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                onClicked: { applyTimer.stop(); root.act("color " + modelData.k) } }
            }
          }
        }

        Row {
          spacing: Style.spacing.lg
          width: parent.width
          Rectangle {
            width: Style.space(44); height: Style.space(44); radius: Style.space(8)
            color: root.shownHex; border.width: 2; border.color: root.fg
          }
          Column {
            anchors.verticalCenter: parent.verticalCenter
            Label { text: root.shownHex.toUpperCase(); font.pixelSize: Style.font.title }
            Label {
              opacity: 0.6; font.pixelSize: Style.font.caption
              text: root.mode === "theme" ? "follows the theme accent"
                  : (root.mode === "random" ? "new color at each theme change and boot" : "drag a bar to change")
            }
          }
        }

        // random: roll on demand
        Rectangle {
          visible: root.mode === "random"
          width: parent.width; height: Style.space(34); radius: Style.space(6)
          color: "transparent"; border.width: 1; border.color: root.fg
          Label { anchors.centerIn: parent; text: "Roll a new color" }
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.act("random") }
        }

        // custom: the picker
        Column {
          visible: root.mode === "custom"
          width: parent.width
          spacing: Style.spacing.xl

          Label { text: "Color"; opacity: 0.7; font.pixelSize: Style.font.caption }
          ColorBar {
            width: parent.width; value: root.hue
            colorAt: function(t) { return root.hsvToHex(t, 1, 1) }
            onMoved: function(v) { root.hue = Math.min(v, 0.999); root.picked() }
          }
          Label { text: "Vividness"; opacity: 0.7; font.pixelSize: Style.font.caption }
          ColorBar {
            width: parent.width; value: root.sat
            colorAt: function(t) { return root.hsvToHex(root.hue, t, root.val) }
            onMoved: function(v) { root.sat = v; root.picked() }
          }
          Label { text: "Brightness"; opacity: 0.7; font.pixelSize: Style.font.caption }
          ColorBar {
            width: parent.width; value: root.val
            colorAt: function(t) { return root.hsvToHex(root.hue, root.sat, t) }
            onMoved: function(v) { root.val = v; root.picked() }
          }
          Row {
            spacing: Style.spacing.sm
            Repeater {
              model: root.presets
              Rectangle {
                width: Style.space(26); height: Style.space(26); radius: Style.space(6)
                color: modelData; border.width: 1; border.color: root.fg
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                  onClicked: { root.setFromHex(modelData); root.picked() } }
              }
            }
          }
        }

        PanelSeparator { foreground: root.fg }

        PanelSectionHeader { text: "SOUND"; foreground: root.fg; fontFamily: root.fontFam; font.letterSpacing: 1 }
        Row {
          spacing: Style.spacing.sm
          Repeater {
            model: [{ k: "all", t: "All" }, { k: "ignite", t: "Ignite only" }, { k: "mute", t: "Mute" }]
            Rectangle {
              width: Style.space(96); height: Style.space(30); radius: Style.space(6)
              color: root.cfg.sound === modelData.k ? root.shownHex : "transparent"
              border.width: 1; border.color: root.fg
              Label { anchors.centerIn: parent; text: modelData.t
                color: root.cfg.sound === modelData.k ? "#000000" : root.fg }
              MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.act("sound " + modelData.k) }
            }
          }
        }
        Row {
          spacing: Style.spacing.lg
          width: parent.width
          Label { text: "Volume " + Math.round(volumeSlider.liveValue) + "%"; anchors.verticalCenter: parent.verticalCenter; width: Style.space(96) }
          PanelSlider {
            id: volumeSlider
            bar: root.bar
            width: parent.width - Style.space(96) - Style.spacing.lg
            anchors.verticalCenter: parent.verticalCenter
            minimum: 0; maximum: 100; step: 5; integer: true
            value: Number(root.cfg.volume)
            onReleased: function(v) { root.act("volume " + Math.round(v)) }
          }
        }

        PanelSeparator { foreground: root.fg }

        Row {
          width: parent.width; spacing: Style.spacing.lg
          Label { text: "Drag trail"; width: parent.width - trailSwitch.width - Style.spacing.lg; anchors.verticalCenter: parent.verticalCenter }
          ToggleSwitch {
            id: trailSwitch
            checked: root.cfg.trail === "on"
            accent: root.shownHex
            onToggled: root.act("trail " + (root.cfg.trail === "on" ? "off" : "on"))
          }
        }
        Row {
          width: parent.width; spacing: Style.spacing.lg
          Label { text: "Kyber cursor"; width: parent.width - activeSwitch.width - Style.spacing.lg; anchors.verticalCenter: parent.verticalCenter }
          ToggleSwitch {
            id: activeSwitch
            checked: root.cfg.active === true
            accent: root.shownHex
            onToggled: root.act(root.cfg.active === true ? "off" : "apply")
          }
        }
      }
    }
  }
}
// qmllint enable missing-property
