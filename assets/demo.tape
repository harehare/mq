Output "demo.gif"

Require echo

Set Shell "zsh"
Set FontSize 24
Set Width 1200
Set Height 600
Set TypingSpeed 50ms
Set FontFamily "Hack Nerd Font"
Set Padding 16
Set WindowBar Colorful
Set Framerate 20

Set Theme { "name": "mq", "black": "#2a3444", "red": "#ef4444", "green": "#34d399", "yellow": "#f59e0b", "blue": "#67b8e3", "magenta": "#a78bfa", "cyan": "#4fc3f7", "white": "#e2e8f0", "brightBlack": "#4a5568", "brightRed": "#ef4444", "brightGreen": "#34d399", "brightYellow": "#f59e0b", "brightBlue": "#67b8e3", "brightMagenta": "#a78bfa", "brightCyan": "#4fc3f7", "brightWhite": "#f8fafc", "background": "#1e293b", "foreground": "#e2e8f0", "selection": "#4a5568", "cursor": "#67b8e3" }

Hide
Type "clear"
Enter
Show

Type `mq '.h' demo.md`
Enter
Sleep 1.5s

Type `mq '.h | select(contains("Filter"))' demo.md`
Enter
Sleep 1.5s

Type `mq 'nodes | .code | compact() | first()' example.html`
Enter
Sleep 1.5s

Type `mq '.h3' demo.md`
Enter
Sleep 1.5s

Type `mq 'select(.code.lang == "rust")' demo.md`
Enter
Sleep 1.5s

Type `mq 'select(.code.lang == "rust") | to_text()' demo.md`
Enter
Sleep 1.5s

Type `mq '.[]' demo.md`
Enter
Sleep 1.5s

Type `mq '.[][]' demo.md`
Enter
Sleep 1.5s

Type `mq -U '.[1][1] | upcase()' demo.md`
Enter
Sleep 1.5s
