#!/bin/bash
# shot.sh NAME: a screenshot of the test GNOME, saved on the host as /tmp/gnome-NAME.png.
docker exec sunna-gnome bash -c ". /tmp/env.sh; gdbus call --session --dest org.gnome.Shell.Screenshot --object-path /org/gnome/Shell/Screenshot --method org.gnome.Shell.Screenshot.Screenshot false false /tmp/$1.png >/dev/null" &&
  docker cp sunna-gnome:/tmp/$1.png /tmp/gnome-$1.png >/dev/null && echo /tmp/gnome-$1.png
