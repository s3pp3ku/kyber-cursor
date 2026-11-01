// qmllint disable missing-property
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// The Kyber Cursor popup: pick the glow color with a hue / saturation /
// brightness bar, choose the sound, set the volume, switch the drag trail and
// the cursor itself on or off. Every change goes through the `kyber` command
// (the bar widget's act()), so it matches the keybinds and the terminal.
// With Plume installed it also sets Plume's pointer trail, through Plume's IPC.
Panel {
  id: root
  moduleName: "io.github.s3pp3ku.kyber-cursor"
  ipcTarget: "io.github.s3pp3ku.kyber-cursor"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root

  readonly property color fg: bar ? bar.foreground : Commons.Color.foreground
  readonly property string fontFam: bar ? bar.fontFamily : Style.font.family
  readonly property var cfg: hostWidget ? hostWidget.cfg : ({ active: false, color: "theme", sound: "all", trail: "on", aim: "off", aimMirror: "on", volume: 55 })
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
    : (mode === "random" && /^#[0-9a-fA-F]{6}$/.test(String(cfg.random)) ? String(cfg.random) : colorToHex(Commons.Color.accent))

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
    var saved = /^#[0-9a-fA-F]{6}$/.test(String(cfg.custom)) ? String(cfg.custom) : colorToHex(Commons.Color.accent)
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
  // the effects preset, worked out from the settings it sets; "" when the
  // user has mixed their own under More settings
  readonly property string effects: cfg.trail === "on" && particles === "sparks" && cfg.sound === "all" ? "full"
    : cfg.trail === "on" && particles === "off" && cfg.sound === "ignite" ? "minimal"
    : cfg.trail !== "on" && cfg.sound === "mute" ? "basic" : ""
  // the trail look; the defaults (comet + sparks) are the preferred look
  readonly property string trailStyle: cfg.style ? String(cfg.style) : "comet"
  readonly property string particles: cfg.particles ? String(cfg.particles) : "sparks"
  readonly property bool defaultLook: trailStyle === "comet" && particles === "sparks"
    && Number(cfg.sparkLife || 850) === 850 && Number(cfg.sparkGap || 20) === 20
    && (cfg.sparkColors || "mixed") === "mixed"
  // "More settings" stays folded unless the user opens it
  property bool moreOpen: false
  // Plume's live settings (null when Plume is not installed) and its setter
  readonly property var plume: hostWidget ? hostWidget.plume : null
  function plumeSet(key, value) { if (hostWidget) hostWidget.plumeSet(key, value) }
  // the Plume trail style: with the blade gate on it is the style worn while lit,
  // otherwise Plume's own trail setting
  readonly property bool plumeGate: hostWidget ? hostWidget.plumeGate : false
  readonly property string plumeStyle: plumeGate ? (hostWidget ? hostWidget.plumeStyle : "sparks")
    : (plume ? String(plume.trail) : "none")
  function pickPlumeStyle(style) {
    if (!hostWidget) return
    if (plumeGate) hostWidget.setPlumeStyle(style)
    else plumeSet("trail", style)
  }

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
      preventStealing: true
      function pick(mx) { cb.moved(Math.max(0, Math.min(1, mx / cb.width))) }
      onPressed: function(m) { pick(m.x) }
      onPositionChanged: function(m) { if (pressed) pick(m.x) }
    }
  }

  // A choice button that grows to fit its text; a ✓ marks the chosen one.
  component Chip: Rectangle {
    id: chip
    property string text
    property bool chosen: false
    signal clicked()
    width: Math.max(Style.space(76), chipLabel.implicitWidth + Style.space(24))
    height: Style.space(30); radius: Style.space(6)
    color: chosen ? root.shownHex : "transparent"
    border.width: chosen ? 2 : 1
    border.color: chosen ? root.shownHex : root.fg
    Label { id: chipLabel; anchors.centerIn: parent; text: (chip.chosen ? "✓ " : "") + chip.text
      color: chip.chosen ? "#000000" : root.fg }
    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: chip.clicked() }
  }

  // Slider captions share one width: the widest any of them can get.
  TextMetrics { id: captionMetrics; font.family: root.fontFam; font.pixelSize: Style.font.body; text: "Bubbles last 2.00s" }
  readonly property real captionW: Math.ceil(captionMetrics.advanceWidth) + Style.space(6)

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
    contentHeight: panel.fittedContentHeight(contentCol.implicitHeight, Style.space(820))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      // scrolls when the settings are taller than the screen allows
      Flickable {
        id: scroller
        anchors.fill: parent
        contentWidth: width
        contentHeight: contentCol.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { id: vScroll; policy: ScrollBar.AsNeeded; width: Style.spacing.sm }

      Column {
        id: contentCol
        // leave room for the scrollbar only when it shows
        width: scroller.width - (scroller.interactive ? vScroll.width + Style.spacing.sm : 0)
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
        Flow {
          width: parent.width
          spacing: Style.spacing.sm
          Repeater {
            model: [{ k: "theme", t: "Theme" }, { k: "custom", t: "Custom" }, { k: "random", t: "Random" }]
            Chip { text: modelData.t; chosen: root.mode === modelData.k
              onClicked: { applyTimer.stop(); root.act("color " + modelData.k) } }
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

        PanelSectionHeader { text: "EFFECTS"; foreground: root.fg; fontFamily: root.fontFam; font.letterSpacing: 1 }
        Label {
          width: parent.width; wrapMode: Text.WordWrap
          text: "Active: " + (root.effects === "full" ? "FULL EFFECTS" : root.effects === "minimal" ? "MINIMAL EFFECTS"
            : root.effects === "basic" ? "BASIC EFFECTS" : "YOUR OWN MIX")
          font.pixelSize: Style.font.title
        }
        Flow {
          width: parent.width
          spacing: Style.spacing.sm
          Repeater {
            model: [{ k: "full", t: "Full" }, { k: "minimal", t: "Minimal" }, { k: "basic", t: "Basic" }]
            Chip { text: modelData.t; chosen: root.effects === modelData.k; onClicked: root.act("effects " + modelData.k) }
          }
        }
        Label {
          width: parent.width; wrapMode: Text.WordWrap
          opacity: 0.7; font.pixelSize: Style.font.caption
          text: root.effects === "full" ? "Glow, drag trail, sparks and every sound."
            : root.effects === "minimal" ? "Glow and drag trail, no sparks, only the ignite sounds."
            : root.effects === "basic" ? "Just the glow. No trail, no sounds."
            : "Changed under More settings. Pick one above to go back."
        }
        Row {
          spacing: Style.spacing.lg
          width: parent.width
          Label { text: "Volume " + Math.round(volumeSlider.liveValue) + "%"; anchors.verticalCenter: parent.verticalCenter; width: root.captionW }
          PanelSlider {
            id: volumeSlider
            bar: root.bar
            width: parent.width - root.captionW - Style.spacing.lg
            anchors.verticalCenter: parent.verticalCenter
            minimum: 0; maximum: 100; step: 5; integer: true
            value: Number(root.cfg.volume)
            onReleased: function(v) { root.act("volume " + Math.round(v)) }
          }
        }

        // everything else, folded away: the stock experience needs none of it
        Label {
          text: (root.moreOpen ? "▾ " : "▸ ") + "More settings"
          opacity: 0.7; font.pixelSize: Style.font.caption
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.moreOpen = !root.moreOpen }
        }
        Column {
          visible: root.moreOpen
          width: parent.width
          spacing: Style.spacing.lg

          Label { text: "Sounds"; opacity: 0.7; font.pixelSize: Style.font.caption }
          Flow {
            width: parent.width
            spacing: Style.spacing.sm
            Repeater {
              model: [{ k: "all", t: "All" }, { k: "ignite", t: "Ignite only" }, { k: "mute", t: "Mute" }]
              Chip { text: modelData.t; chosen: root.cfg.sound === modelData.k; onClicked: root.act("sound " + modelData.k) }
            }
          }
          Row {
            width: parent.width; spacing: Style.spacing.lg
            Label { text: "Kyber drag trail"; width: parent.width - trailSwitch.width - Style.spacing.lg; wrapMode: Text.WordWrap; anchors.verticalCenter: parent.verticalCenter }
            ToggleSwitch {
              id: trailSwitch
              checked: root.cfg.trail === "on"
              accent: root.shownHex
              onToggled: root.act("trail " + (root.cfg.trail === "on" ? "off" : "on"))
            }
          }

          // Aim: the blade turns to face the pointer's direction of travel and
          // keeps its angle when the pointer stops. Only the native overlay
          // runtime can draw that (the XCursor theme is a static bitmap), so
          // the switch simply stays off where that runtime is not installed.
          Row {
            width: parent.width; spacing: Style.spacing.lg
            Label {
              text: "Aim (blade faces your movement)"
              width: parent.width - aimSwitch.width - Style.spacing.lg
              wrapMode: Text.WordWrap
              anchors.verticalCenter: parent.verticalCenter
            }
            ToggleSwitch {
              id: aimSwitch
              checked: root.cfg.aim === "on"
              accent: root.shownHex
              onToggled: root.act("aim " + (root.cfg.aim === "on" ? "off" : "on"))
            }
          }
          Row {
            visible: root.cfg.aim === "on"
            width: parent.width; spacing: Style.spacing.lg
            Label {
              text: "Mirror the lit edge as it turns"
              width: parent.width - aimMirrorSwitch.width - Style.spacing.lg
              wrapMode: Text.WordWrap
              anchors.verticalCenter: parent.verticalCenter
            }
            ToggleSwitch {
              id: aimMirrorSwitch
              checked: (root.cfg.aimMirror || "on") === "on"
              accent: root.shownHex
              onToggled: root.act("aim mirror " + ((root.cfg.aimMirror || "on") === "on" ? "off" : "on"))
            }
          }

          // how long the sparks last and how close together they fly
          Row {
            visible: root.cfg.trail === "on" && root.particles !== "off"
            spacing: Style.spacing.lg
            width: parent.width
            Label { text: (root.particles === "bubbles" ? "Bubbles last " : "Sparks last ") + (lifeSlider.liveValue / 1000).toFixed(2) + "s"
              anchors.verticalCenter: parent.verticalCenter; width: root.captionW }
            PanelSlider {
              id: lifeSlider
              bar: root.bar
              width: parent.width - root.captionW - Style.spacing.lg
              anchors.verticalCenter: parent.verticalCenter
              minimum: 150; maximum: 1500; step: 50; integer: true
              value: Number(root.cfg.sparkLife || 850)
              onReleased: function(v) { root.act("sparks life " + Math.round(v)) }
            }
          }
          Row {
            visible: root.cfg.trail === "on" && root.particles !== "off"
            spacing: Style.spacing.lg
            width: parent.width
            // right is closer together: the slider shows 42 - gap, so 2..40 px
            // becomes 40..2 and dragging right adds sparks
            Label { text: "Close together"; anchors.verticalCenter: parent.verticalCenter; width: root.captionW }
            PanelSlider {
              bar: root.bar
              width: parent.width - root.captionW - Style.spacing.lg
              anchors.verticalCenter: parent.verticalCenter
              minimum: 2; maximum: 40; step: 1; integer: true
              value: 42 - Number(root.cfg.sparkGap || 20)
              onReleased: function(v) { root.act("sparks gap " + (42 - Math.round(v))) }
            }
          }

          Label { text: "Trail shape"; opacity: 0.7; font.pixelSize: Style.font.caption }
          Flow {
            width: parent.width
            spacing: Style.spacing.sm
            Repeater {
              model: [{ k: "comet", t: "Comet" }, { k: "ribbon", t: "Ribbon" }, { k: "beads", t: "Beads" },
                      { k: "echoes", t: "Echoes" }, { k: "none", t: "None" }]
              Chip { text: modelData.t; chosen: root.trailStyle === modelData.k; onClicked: root.act("style " + modelData.k) }
            }
          }
          Label { text: "What flies off it"; opacity: 0.7; font.pixelSize: Style.font.caption }
          Flow {
            width: parent.width
            spacing: Style.spacing.sm
            Repeater {
              model: [{ k: "sparks", t: "Sparks" }, { k: "bubbles", t: "Bubbles" }, { k: "off", t: "Nothing" }]
              Chip { text: modelData.t; chosen: root.particles === modelData.k; onClicked: root.act("sparks " + modelData.k) }
            }
          }
          // mixed: mostly hot orange-red sparks, a quarter in the glow color
          Label { visible: root.particles === "sparks"; text: "Spark colors"; opacity: 0.7; font.pixelSize: Style.font.caption }
          Flow {
            visible: root.particles === "sparks"
            width: parent.width
            spacing: Style.spacing.sm
            Repeater {
              model: [{ k: "mixed", t: "Mixed" }, { k: "glow", t: "Glow color" }]
              Chip { text: modelData.t; chosen: (root.cfg.sparkColors || "mixed") === modelData.k; onClicked: root.act("sparks colors " + modelData.k) }
            }
          }
          Rectangle {
            visible: !root.defaultLook
            width: resetLabel.implicitWidth + Style.space(24); height: Style.space(30); radius: Style.space(6)
            color: "transparent"; border.width: 1; border.color: root.fg
            Label { id: resetLabel; anchors.centerIn: parent; text: "Reset the trail look" }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.act("sparks default") }
          }

          // Plume's pointer trail, when Plume is installed (see KyberWidget's bridge).
          // Kyber draws its own sparks now, so Plume is optional.
          Column {
            visible: root.plume !== null
            width: parent.width
            spacing: Style.spacing.lg

            Label { text: "Plume trail"; opacity: 0.7; font.pixelSize: Style.font.caption }
            Flow {
              width: parent.width
              spacing: Style.spacing.sm
              Repeater {
                model: [{ k: "none", t: "Off" }, { k: "sparks", t: "Sparks" }, { k: "comet", t: "Comet" }, { k: "ribbon", t: "Ribbon" },
                        { k: "beads", t: "Beads" }, { k: "echoes", t: "Echoes" }, { k: "bubbles", t: "Bubbles" }]
                Chip { text: modelData.t; chosen: root.plumeStyle === modelData.k; onClicked: root.pickPlumeStyle(modelData.k) }
              }
            }
            Repeater {
              // Plume's own ranges: trailSize 2..30, trailLife 15..150 (hundredths of a second)
              // captions name what each slider changes and show its value while dragging
              model: [{ k: "trailSize", min: 2, max: 30, step: 1 }, { k: "trailLife", min: 15, max: 150, step: 5 }]
              Row {
                visible: root.plumeStyle !== "none"
                width: parent.width; spacing: Style.spacing.lg
                Label {
                  text: modelData.k === "trailSize"
                    ? "Trail width " + Math.round(plumeSlider.liveValue) + "px"
                    : "Trail lasts " + (plumeSlider.liveValue / 100).toFixed(2) + "s"
                  anchors.verticalCenter: parent.verticalCenter; width: root.captionW
                }
                PanelSlider {
                  id: plumeSlider
                  bar: root.bar
                  width: parent.width - root.captionW - Style.spacing.lg
                  anchors.verticalCenter: parent.verticalCenter
                  minimum: modelData.min; maximum: modelData.max; step: modelData.step; integer: true
                  value: root.plume ? Number(root.plume[modelData.k]) : modelData.min
                  onReleased: function(v) { root.plumeSet(modelData.k, Math.round(v)) }
                }
              }
            }
            Row {
              width: parent.width; spacing: Style.spacing.lg
              Label { text: "Only while the blade is lit"; width: parent.width - gateSwitch.width - Style.spacing.lg; wrapMode: Text.WordWrap; anchors.verticalCenter: parent.verticalCenter }
              ToggleSwitch {
                id: gateSwitch
                checked: root.hostWidget ? root.hostWidget.plumeGate : false
                accent: root.shownHex
                onToggled: if (root.hostWidget) root.hostWidget.setPlumeGate(!root.hostWidget.plumeGate)
              }
            }
            Row {
              width: parent.width; spacing: Style.spacing.lg
              Label { text: "Plume follows the glow color"; width: parent.width - syncSwitch.width - Style.spacing.lg; wrapMode: Text.WordWrap; anchors.verticalCenter: parent.verticalCenter }
              ToggleSwitch {
                id: syncSwitch
                checked: root.hostWidget ? root.hostWidget.plumeSync : false
                accent: root.shownHex
                onToggled: if (root.hostWidget) root.hostWidget.setPlumeSync(!root.hostWidget.plumeSync)
              }
            }
          }
        }

        PanelSeparator { foreground: root.fg }
        Row {
          width: parent.width; spacing: Style.spacing.lg
          Label { text: "Kyber cursor"; width: parent.width - activeSwitch.width - Style.spacing.lg; wrapMode: Text.WordWrap; anchors.verticalCenter: parent.verticalCenter }
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
}
// qmllint enable missing-property
