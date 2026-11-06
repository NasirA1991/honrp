honrp — Windows alpha
=====================

This is an untested alpha build. It is portable: there is no installer, no
admin requirement, and no Program Files install.

Setup
-----
1. Extract this zip anywhere you can write (for example a folder in your user
   profile; do not put it in Program Files).
2. Run:

       honrp.exe install

   This writes the Lua bridge into:

       %USERPROFILE%\Documents\Juvio\Heroes of Newerth\startup.lua

   It does not touch the Heroes of Newerth install directory and does not
   require administrator rights. If detection fails, pass the folder:

       honrp.exe install --user-dir "%USERPROFILE%\Documents\Juvio\Heroes of Newerth"

3. Start the daemon while you play:

       honrp.exe run

   Keep that window open. Autostart is not included in this alpha.

4. Restart Heroes of Newerth once so the engine loads the bridge.

Useful commands
---------------
    honrp.exe status [--json]
    honrp.exe doctor
    honrp.exe run -v
    honrp.exe uninstall

Discord
-------
The Discord application id is built in. Discord must be running. If Discord
also lists "juvio.exe" as a separate game, disable it once in
Settings -> Game Activity -> Registered Games; honrp cannot change that list.

SmartScreen
-----------
The binary is not code-signed. Windows may show "Windows protected your PC";
use "More info" -> "Run anyway". You can also unblock the zip before extracting
(right-click -> Properties -> Unblock).

Feedback
--------
Please include the output of `honrp.exe doctor`, your Windows version, how you
launch HoN (Steam/non-Steam/Lutris-style), and your Discord client. This build
has not been tested on real Windows hardware yet.
