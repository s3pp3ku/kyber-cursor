// qmllint disable missing-property
import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// A bar button for Kyber Cursor. Click it to open a popup with a color picker
// and the sound, volume, trail and on/off options.
//   left click:   open the popup
//   middle click: cycle the effects (full -> minimal -> basic)
//   right click:  Kyber cursor on / off
// Everything is stored by the `kyber` command, so a keybind, the terminal and
// this button always agree. The button only shows what `kyber config` reports.
BarWidget {
  id: root

  // "collapsed": true on the bar entry keeps this widget live on the bar (its
  // service and panels still work) but takes no space; a drawer launches it.
  Binding {
    target: root
    property: "visible"
    value: false
    when: !!root.settings && root.settings.collapsed === true
  }
  moduleName: "io.github.s3pp3ku.kyber-cursor"

  readonly property string pluginDir: Qt.resolvedUrl(".").toString().replace("file://", "")
  readonly property string kyber: Util.shellQuote(pluginDir + "bin/kyber")

  property var cfg: ({ active: false, color: "theme", sound: "all", trail: "on", aim: "off", aimMirror: "on", volume: 55 })
  property var lastSettings: null
  readonly property bool customColor: /^#[0-9a-fA-F]{6}$/.test(String(cfg.color))
  readonly property color effectiveColor: {
    if (cfg.mode === "random" && /^#[0-9a-fA-F]{6}$/.test(String(cfg.random))) return String(cfg.random)
    return root.customColor ? String(cfg.color) : Commons.Color.accent
  }

  readonly property string stateDir: (Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/kyber-cursor"

  // the effects preset the settings add up to, or "" for the user's own mix
  readonly property string effects: cfg.trail === "on" && cfg.particles === "sparks" && cfg.sound === "all" ? "full"
    : cfg.trail === "on" && cfg.particles === "off" && cfg.sound === "ignite" ? "minimal"
    : cfg.trail !== "on" && cfg.sound === "mute" ? "basic" : ""

  function refresh() { if (!readProc.running) readProc.running = true }

  function act(args) {
    root.bar.run(root.kyber + " " + args)
    refreshTimer.restart()
  }

  // The settings panel only pushes what the user changed since the widget
  // loaded, so reloading the shell never overwrites a value set with the
  // `kyber` command or a keybind.
  function pushSettings() {
    var s = root.settings || ({})
    var prev = root.lastSettings
    root.lastSettings = JSON.parse(JSON.stringify(s))
    if (prev === null) return
    if (s.colorMode !== prev.colorMode && s.colorMode) act("color " + Util.shellQuote(String(s.colorMode)))
    if (s.color !== prev.color) {
      var c = String(s.color || "").trim()
      if (/^#?[0-9a-fA-F]{6}$/.test(c)) act("color " + Util.shellQuote(c))
    }
    if (s.sound !== prev.sound && s.sound) act("sound " + Util.shellQuote(String(s.sound)))
    if (s.trail !== prev.trail) act("trail " + (s.trail === false ? "off" : "on"))
    if (s.volume !== prev.volume && s.volume !== undefined) act("volume " + parseInt(s.volume))
  }

  onSettingsChanged: { pushSettings(); injectPanel() }
  Component.onCompleted: { pushSettings(); refresh() }

  // ---- popup panel (shape contract for the shell's open/close/toggle)
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  function open() {
    refresh()
    if (panelLoader.item) panelLoader.item.open()
  }
  function close() { if (panelLoader.item) panelLoader.item.close() }
  function togglePanel() { if (panelLoader.item) panelLoader.item.toggle() }

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }
  onBarChanged: injectPanel()

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: { root.injectPanel(); Qt.callLater(root.injectPanel) }
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Process {
    id: readProc
    // the config JSON, then the trail look (style, particles, spark life ms,
    // spark gap px, spark colors)
    command: ["bash", "-c", root.kyber + " config; cd " + Util.shellQuote(root.stateDir) + " 2>/dev/null; echo"
      + "; cat trail-style 2>/dev/null || echo comet"
      + "; cat trail-particles 2>/dev/null || echo sparks"
      + "; cat spark-life 2>/dev/null || echo 850"
      + "; cat spark-gap 2>/dev/null || echo 20"
      + "; cat spark-colors 2>/dev/null || echo mixed"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var lines = text.trim().split("\n")
        var colors = lines.pop().trim()
        var gap = parseInt(lines.pop()), life = parseInt(lines.pop())
        var particles = lines.pop().trim(), style = lines.pop().trim()
        try {
          var c = JSON.parse(lines.join("\n").trim())
          c.style = /^(comet|none)$/.test(style) ? style : "comet"
          c.particles = /^(sparks|off)$/.test(particles) ? particles : "sparks"
          c.sparkLife = life >= 150 && life <= 1500 ? life : 850
          c.sparkGap = gap >= 2 && gap <= 40 ? gap : 20
          c.sparkColors = colors === "glow" ? "glow" : "mixed"
          root.cfg = c
        } catch (e) { }
      }
    }
  }

  Timer { id: refreshTimer; interval: 250; onTriggered: root.refresh() }
  // pick up changes made with the keybinds or the terminal
  Timer { interval: 4000; running: true; repeat: true; onTriggered: root.refresh() }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: ""
    active: root.cfg.active === true
    activeColor: root.effectiveColor
    dimmed: root.cfg.active !== true
    tooltipText: "Kyber cursor: " + (root.cfg.active === true ? "on" : "off")
      + "\nGlow color: " + (root.cfg.mode === "random" ? "random " + root.cfg.color
        : (root.customColor ? root.cfg.color : "follows the theme"))
      + "\nEffects: " + (root.effects || "your own mix") + " (volume " + root.cfg.volume + ")"
      + "\nAim: " + (root.cfg.aim === "on" ? "on (mirror " + (root.cfg.aimMirror || "on") + ")" : "off")
      + "\nLeft click: options · Middle: effects · Right: on/off"

    onPressed: function(buttonCode) {
      if (buttonCode === Qt.MiddleButton) root.act("effects " + (root.effects === "full" ? "minimal" : root.effects === "minimal" ? "basic" : "full"))
      else if (buttonCode === Qt.RightButton) root.act(root.cfg.active === true ? "off" : "apply")
      else root.togglePanel()
    }
  }
}
// qmllint enable missing-property
