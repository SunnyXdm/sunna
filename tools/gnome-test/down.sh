#!/bin/bash
# Remove the test container (the image stays; `docker rmi sunna-gnome-test` removes it too).
docker rm -f sunna-gnome >/dev/null 2>&1 && echo "removed the sunna-gnome container" || echo "no sunna-gnome container"
