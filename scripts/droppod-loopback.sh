#!/bin/sh
# Project Reclaimer under Wine binds 127.0.0.x, 127.0.1.x, 127.0.2.x (0.9.8+), 127.3.1.x and 127.3.2.x (0.9.13);
# macOS only has 127.0.0.1 on lo0 by default. 127.0.3.x and 127.3.3.x are spare room for the next range a release adds.
for p in 127.0.0 127.0.1 127.0.2 127.0.3 127.3.1 127.3.2 127.3.3; do
  i=1
  while [ $i -le 254 ]; do
    [ "$p.$i" = 127.0.0.1 ] && { i=$((i+1)); continue; }
    /sbin/ifconfig lo0 alias "$p.$i" netmask 255.255.255.255 2>/dev/null
    i=$((i+1))
  done
done
