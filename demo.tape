# Records the README's demo GIF with VHS (https://github.com/charmbracelet/vhs):
#
#   cargo build --release && vhs demo.tape
#
# Writes demo.gif. Re-record it whenever the look changes.

Output demo.gif

Set Shell "bash"
Set FontSize 15
Set Width 900
Set Height 560
Set Padding 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Hide
Type "export PATH=$PWD/target/release:$PATH TINSEL_NO_HISTORY=1 TINSEL_NO_FOCUS=1 && clear"
Enter
Show

Type "tinsel demo"
Sleep 400ms
Enter
Sleep 13s

Hide
Type "clear"
Enter
Show

Type "tinsel showcase report"
Sleep 400ms
Enter
Sleep 3s
