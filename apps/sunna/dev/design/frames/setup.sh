#!/bin/bash
set -eu
# Run inside the disposable Debian 13 container after installing packages in README.
useradd -m -s /bin/bash sunny || true
mkdir -p /run/dbus
 dbus-daemon --system --fork || true
for user in sunny gnome xfce mate; do
 id "$user" >/dev/null 2>&1 || useradd -m -s /bin/bash "$user"
 home=/home/$user
 mkdir -p "$home"/{Desktop,Documents,Downloads,Music,Pictures,Projects,Videos,.config,.local/share}
 cp /tmp/linux.rs "$home/Projects/linux.rs"
 cp /tmp/earthrise.jpg "$home/Pictures/Earthrise.jpg"
 cat > "$home/Documents/Capture-notes.txt" <<'NOTES'
Sunna / capture notes
Tuesday, 17 November

The quiet frames matter as much as the busy ones.

Today
  • Review the X11 shared-memory capture path
  • Check idle frame delivery after reconnecting
  • Compare full-frame and tiled updates

The capture loop should remain predictable when the
remote computer is idle. Keep the last frame visible
while the next one is on its way.

Next session
  Try a wide creative workspace and a small laptop.
  Check the transition between two live previews.
NOTES
 cat > "$home/.bashrc" <<'RC'
export PS1='\[\e[32m\]sunny@debian\[\e[0m\]:\[\e[34m\]\w\[\e[0m\]\$ '
export TERM=xterm-256color
alias ls='ls --color=auto'
RC
 chown -R "$user:$user" "$home"
 uid=$(id -u "$user")
 mkdir -p /run/user/$uid
 chown "$user:$user" /run/user/$uid
 chmod 700 /run/user/$uid
done
cat > /home/sunny/.config/kdeglobals <<'CONF'
[General]
ColorScheme=BreezeDark
font=Noto Sans,11,-1,5,50,0,0,0,0,0
[Icons]
Theme=breeze-dark
[KDE]
LookAndFeelPackage=org.kde.breezedark.desktop
CONF
chown sunny:sunny /home/sunny/.config/kdeglobals
