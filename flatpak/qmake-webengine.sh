#!/bin/sh
# The Qt of the SDK is in /usr and the QtWebEngine of the base app is in /app,
# but cxx-qt only looks for libraries where qmake says they are. Report the
# folder that the build fills with links to both.
if [ "$1" = "-query" ] && [ "$2" = "QT_INSTALL_LIBS" ]; then
  echo "${QT_MERGED_LIBS:-/run/build/mailviewer-kde/qtlibs}"
  exit 0
fi
exec /usr/bin/qmake6 "$@"
