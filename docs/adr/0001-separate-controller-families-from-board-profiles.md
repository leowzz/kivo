# Separate Controller Families From Board Profiles

Kivo separates shared MCU-platform behavior into Controller Families and
concrete USB identity, GPIO capabilities, and firmware targets into Board
Profiles. Hardware Profiles target a Board Profile, Device IDs combine Board
Profile with hardware serial, and protocol v3 reports both identities; this
keeps support for another board on an existing MCU independent from adding a
new MCU platform.
