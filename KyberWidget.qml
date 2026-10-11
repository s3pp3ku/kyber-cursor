// qmllint disable missing-property
import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui

// A bar button for Kyber Cursor. Click it to open a popup with a color picker
// and the sound, volume, trail and on/off options.
//   left click:   open the popup
//   middle click: cycle the sound  (all -> ignite -> mute)
//   right click:  Kyber cursor on / off
// Everything is stored by the `kyber` command, so a keybind, the terminal and
// this button always agree. The button only shows what `kyber config` reports.
BarWidget {
  id: root
  moduleName: "io.github.s3pp3ku.kyber-cursor"

  readonly property string pluginDir: Qt.resolvedUrl(".").toString().replace("file://", "")
  readonly property string kyber: Util.shellQuote(pluginDir + "bin/kyber")

  property var cfg: ({ active: false, color: "theme", sound: "all", trail: "on", volume: 55 })
  property var lastSettings: null
  readonly property bool customColor: /^#[0-9a-fA-F]{6}$/.test(String(cfg.color))

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
    if (s.color !== prev.color) {
      var c = String(s.color || "").trim()
      act("color " + Util.shellQuote(c === "" ? "theme" : c))
    }
    if (s.sound !== prev.sound && s.sound) act("sound " + Util.shellQuote(String(s.sound)))
    if (s.trail !== prev.trail) act("trail " + (s.trail === false ? "off" : "on"))
    if (s.volume !== prev.volume && s.volume !== undefined) act("volume " + parseInt(s.volume))
  }

  onSettingsChanged: { pushSettings(); injectPanel() }
  Component.onCompleted: { pushSettings(); refresh() }

  // ---- popup panel (shape contract for the shell's open/close/toggle)
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  function open() { if (panelLoader.item) panelLoader.item.open() }
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
    command: ["bash", "-c", root.kyber + " config"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try { root.cfg = JSON.parse(text.trim()) } catch (e) { }
      }
    }
  }

  Timer { id: refreshTimer; interval: 700; onTriggered: root.refresh() }
  // pick up changes made with the keybinds or the terminal
  Timer { interval: 4000; running: true; repeat: true; onTriggered: root.refresh() }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: ""
    active: root.cfg.active === true
    activeColor: root.customColor ? String(root.cfg.color) : "#a78bfa"
    dimmed: root.cfg.active !== true
    tooltipText: "Kyber cursor: " + (root.cfg.active === true ? "on" : "off")
      + "\nGlow color: " + (root.customColor ? root.cfg.color : "follows the theme")
      + "\nSound: " + root.cfg.sound + " (volume " + root.cfg.volume + ")"
      + "\nDrag trail: " + root.cfg.trail
      + "\nLeft click: options · Middle: sound · Right: on/off"

    onPressed: function(buttonCode) {
      if (buttonCode === Qt.MiddleButton) root.act("sound")
      else if (buttonCode === Qt.RightButton) root.act(root.cfg.active === true ? "off" : "apply")
      else root.togglePanel()
    }
  }
}
// qmllint enable missing-property
