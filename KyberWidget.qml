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

  // ---- Plume bridge: Plume (io.github.qempexe.plume) draws pointer trails.
  // Its settings file is read live; changes go through its `set` IPC, so
  // Plume itself stays unmodified. Null when Plume is not installed.
  readonly property string plumeId: "io.github.qempexe.plume"
  readonly property string stateDir: (Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/kyber-cursor"
  property var plume: null
  // when on, Plume's trail wears the Kyber glow color (flag file: plume-sync)
  property bool plumeSync: false
  readonly property string glowHex: colorToHex(effectiveColor)

  function colorToHex(c) {
    function hx(x) { var n = Math.round(x * 255).toString(16); return n.length < 2 ? "0" + n : n }
    var q = Qt.color(c)
    return "#" + hx(q.r) + hx(q.g) + hx(q.b)
  }

  function plumeSet(key, value) {
    root.bar.run("omarchy-shell " + root.plumeId + " set " + Util.shellQuote(key) + " " + Util.shellQuote(String(value)))
  }

  // Which Plume color settings mirror a Kyber glow color. Plume's color
  // modes: "custom" (one color) or "gradient" (customColor -> customColor2).
  function plumeColors(hex) {
    // TODO(owner): one flat color, or a gradient (say, the glow fading to a
    // lighter Qt.lighter(hex, 1.6) or white-hot tip)? Return the settings.
    return { colorMode: "custom", customColor: hex }
  }

  function pushPlumeColor() {
    if (!root.plumeSync || !root.plume) return
    var want = plumeColors(root.glowHex)
    for (var k in want) if (String(root.plume[k]).toLowerCase() !== String(want[k]).toLowerCase()) plumeSet(k, want[k])
  }
  onGlowHexChanged: pushPlumeColor()
  onPlumeSyncChanged: pushPlumeColor()

  function setPlumeSync(on) {
    root.bar.run((on ? "mkdir -p " + Util.shellQuote(root.stateDir) + " && touch " : "rm -f ") + Util.shellQuote(root.stateDir + "/plume-sync"))
    root.plumeSync = on
  }

  // ---- spark gate: Plume's trail only while the blade is lit. The native
  // adapter posts `kyberbutton>>down,N` / `up,N` / `cancel` on Hyprland's event
  // socket; while any button is held Plume wears plumeStyle, else "none".
  // Flag file: plume-gate; the style used while lit: plume-trail.
  property bool plumeGate: false
  property string plumeStyle: "sparks"
  property var heldButtons: ({})
  property int heldCount: 0
  readonly property string gateWant: !plumeGate ? "" : (heldCount > 0 && cfg.active === true ? plumeStyle : "none")
  property string gateSent: ""
  onGateWantChanged: applyGate()

  Connections {
    target: Hyprland
    function onRawEvent(event) {
      if (event.name !== "kyberbutton") return
      var parts = String(event.data).split(",")
      var held = root.heldButtons
      if (parts[0] === "down") held[parts[1]] = true
      else if (parts[0] === "up") delete held[parts[1]]
      else if (parts[0] === "cancel") held = {}
      else return
      root.heldButtons = held
      root.heldCount = Object.keys(held).length
    }
  }

  // one call at a time, always ending on the latest wish, so a quick click
  // can never leave the sparks stuck on
  Process {
    id: gateProc
    onExited: root.applyGate()
  }
  function applyGate() {
    if (root.gateWant === "" || root.gateWant === root.gateSent || gateProc.running) return
    root.gateSent = root.gateWant
    gateProc.command = ["omarchy-shell", "-q", root.plumeId, "set", "trail", root.gateWant]
    gateProc.running = true
  }

  function writeState(name, text) {
    var f = Util.shellQuote(root.stateDir + "/" + name)
    root.bar.run("mkdir -p " + Util.shellQuote(root.stateDir) + " && " + (text === null ? "rm -f " + f : "printf %s " + Util.shellQuote(text) + " > " + f))
  }
  function setPlumeGate(on) {
    if (on && root.plume && root.plume.trail !== "none") setPlumeStyle(root.plume.trail)
    writeState("plume-gate", on ? "on" : null)
    root.plumeGate = on
    root.gateSent = ""
    if (on) applyGate()
    else plumeSet("trail", root.plumeStyle)
  }
  function setPlumeStyle(style) {
    writeState("plume-trail", style)
    root.plumeStyle = style
  }

  FileView {
    path: Quickshell.env("HOME") + "/.local/state/omarchy-plume/settings.json"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: { try { root.plume = JSON.parse(text()); root.pushPlumeColor() } catch (e) { } }
    onLoadFailed: root.plume = null
  }

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
    // spark gap px, spark colors), then three lines of Plume bridge state: the color
    // sync, the spark gate, and the trail style used while the blade is lit
    command: ["bash", "-c", root.kyber + " config; cd " + Util.shellQuote(root.stateDir) + " 2>/dev/null; echo"
      + "; cat trail-style 2>/dev/null || echo comet"
      + "; cat trail-particles 2>/dev/null || echo sparks"
      + "; cat spark-life 2>/dev/null || echo 850"
      + "; cat spark-gap 2>/dev/null || echo 20"
      + "; cat spark-colors 2>/dev/null || echo mixed"
      + "; [ -e plume-sync ] && echo sync || echo nosync"
      + "; [ -e plume-gate ] && echo gate || echo nogate"
      + "; cat plume-trail 2>/dev/null || echo sparks"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var lines = text.trim().split("\n")
        var style = lines.pop().trim()
        if (/^(none|comet|beads|ribbon|sparks|echoes|bubbles)$/.test(style)) root.plumeStyle = style
        root.plumeGate = lines.pop().trim() === "gate"
        root.plumeSync = lines.pop().trim() === "sync"
        var colors = lines.pop().trim()
        var gap = parseInt(lines.pop()), life = parseInt(lines.pop())
        var particles = lines.pop().trim(), style = lines.pop().trim()
        try {
          var c = JSON.parse(lines.join("\n").trim())
          c.style = /^(comet|ribbon|beads|echoes|none)$/.test(style) ? style : "comet"
          c.particles = /^(sparks|bubbles|off)$/.test(particles) ? particles : "sparks"
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
