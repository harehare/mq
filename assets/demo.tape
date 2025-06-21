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

Type `curl -s https://mqlang.org/book/start/example | mq -I html 'nodes | .code | compact() | first()'`
Enter
Sleep 1.5s

Type `mq '.h3' demo.md`
Enter
Sleep 1.5s

Type `mq '.code("rust")' demo.md`
Enter
Sleep 1.5s

Type `mq '.code("rust") | to_text()' demo.md`
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
