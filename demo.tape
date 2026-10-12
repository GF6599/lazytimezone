# lazytimezone demo, recorded with vhs (https://github.com/charmbracelet/vhs)
# Usage: just demo

Output doc/demo.gif

Set FontSize 14
Set Width 1400
Set Height 800
Set Theme "Rose Pine"
Set Framerate 30
Set TypingSpeed 50ms

# The recording must not read or write the real user config. Env comes
# after every Set, because vhs ignores a Set that follows any command.
Env XDG_CONFIG_HOME "/tmp/lazytimezone-demo-config"

Type "./target/release/lazytimezone"
Enter
Sleep 2s

# Build a wall through the four search forms: city, offset,
# timezone phrase, then a second city
Type "/"
Sleep 800ms
Type "tokyo"
Sleep 1500ms
Enter
Sleep 1s

Type "/"
Sleep 500ms
Type "+5:30"
Sleep 1500ms
Enter
Sleep 1s

Type "/"
Sleep 500ms
Type "eastern time"
Sleep 1500ms
Enter
Sleep 1s

Type "/"
Sleep 500ms
Type "berlin"
Sleep 1500ms
Enter
Sleep 1500ms

# Walk the wall and promote a panel to the big clock
Type "l"
Sleep 600ms
Type "l"
Sleep 600ms
Enter
Sleep 2s

# Cycle two themes
Type "t"
Sleep 1200ms
Type "t"
Sleep 1500ms

Type "q"
Sleep 500ms
