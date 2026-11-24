# An APK PROFILE: what one person's Android portal is built with, read from a
# TOML file and refused at evaluation when it is not one. The generic profile
# (android/profile/generic.toml) is empty.
#
#   box    = "<name>.<tailnet>.ts.net"   the box the portal opens, asked at
#                                        first launch when absent
#   sender = "<digits>"                  that box's Firebase project number;
#                                        absent, the portal registers for no push
#
# The result is a sum: { box = null; } for the generic APK, or
# { box = { host; sender; }; } with sender null for a box that sends no push.
{ lib }:
file:
let
  raw = builtins.fromTOML (builtins.readFile file);
  fields = [ "box" "sender" ];
  unknown = lib.subtractLists fields (builtins.attrNames raw);
  label = "[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?";
  isHost =
    text:
    builtins.isString text
    && builtins.stringLength text <= 253
    && builtins.match "${label}(\\.${label})+" text != null;
  isSender = text: builtins.isString text && builtins.match "[0-9]{1,20}" text != null;
  refuse = why: throw "the APK profile ${toString file}: ${why}";
in
if unknown != [ ] then
  refuse "${lib.concatStringsSep ", " unknown} is no field; the fields are ${lib.concatStringsSep ", " fields}"
else if raw ? box && !(isHost raw.box) then
  refuse "box is a DNS name in lower case, at most 253 characters, with no scheme, port or path"
else if raw ? sender && !(raw ? box) then
  refuse "a sender is its box's Firebase project, so it needs the box"
else if raw ? sender && !(isSender raw.sender) then
  refuse "sender is the Firebase project's number, 1 to 20 digits, as a string"
else if raw ? box then
  { box = { host = raw.box; sender = raw.sender or null; }; }
else
  { box = null; }
