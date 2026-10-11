import QtQuick
import Quickshell.Io

// No UI. Re-applies the cursor once when the shell (re)loads the plugin so a
// fresh `omarchy plugin add --enable` shows the cursor immediately. The
// recoloring on each `omarchy theme set` and at boot comes from the hooks that
// ./install.sh adds (opt-in); the click glow is a systemd user service that
// install.sh also adds. Nothing here runs a build unless Kyber was
// already applied once (`kyber apply`).
Item {
  id: root

  readonly property string pluginDir: Qt.resolvedUrl(".").toString().replace("file://", "")

  Component.onCompleted: applyProcess.running = true

  Process {
    id: applyProcess
    running: false
    command: ["bash", "-c",
      "[ -f \"${XDG_CONFIG_HOME:-$HOME/.config}/kyber-cursor/active\" ] && exec '" + root.pluginDir + "bin/kyber' apply || true"
    ]
    stdout: StdioCollector { onStreamFinished: if (text) console.log("kyber-cursor:", text.trim()) }
    stderr: StdioCollector { onStreamFinished: if (text) console.warn("kyber-cursor:", text.trim()) }
  }
}
