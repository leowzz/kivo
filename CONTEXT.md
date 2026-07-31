# Kivo

Kivo configures physical keypad products and runs their button actions through
connected controller devices. This glossary separates Device Profiles,
editing, runtime selection, and controller identity.

## Language

**Device Profile**:
An assignable keypad or telephone configuration containing its visible layout,
button definitions, button actions, and one or more Hardware Profiles.
_Avoid_: Model, Device, Controller Profile, configuration file

**Editor Profile**:
The one **Device Profile** currently visible and editable in the Kivo workspace.
Changing the Editor Profile does not deactivate any Device's Runtime Assignment.
_Avoid_: Active model, Editor Model, selected device

**Device**:
One individually identifiable physical controller unit. Multiple Devices may
use the same Board Profile and keep different Runtime Assignments.
_Avoid_: Controller, port

**Device ID**:
The stable identity of one **Device**, composed from its Board Profile and unique
hardware serial. A connection port is never part of Device identity.
_Avoid_: Port, USB path, device name

**Device Mode**:
The current USB operating mode of a present **Device**: runtime or bootloader.
Changing mode does not create a new Device or change its Runtime Assignment.
_Avoid_: Device type, connection status

**Device Status**:
The management view derived from a Device's separate connection, identity,
mode, assignment, and runtime states. A primary UI label never replaces those
source states or hides a repairable error.
_Avoid_: Connection status, controller state

**Enrollment**:
The first successful recognition of a valid **Device**, which adds it to Device
Management without a Runtime Assignment. Enrollment is automatic after identity
and protocol validation.
_Avoid_: Pairing, connection

**Forget Device**:
Remove one disconnected **Device** from Device Management together with its
name and Runtime Assignment. Historical metrics and activity remain attributed
to its Device ID; reconnecting enrolls it again without an assignment.
_Avoid_: Disconnect, Delete Device History

**Runtime Assignment**:
The association of one **Device Profile** and one compatible **Hardware
Profile** with one **Device** for live input and action execution. Every Device
keeps its own Runtime Assignment, and Kivo never silently retargets it to a
different Hardware Profile. A valid Runtime Assignment activates automatically
when its Device connects.
_Avoid_: Active model, current configuration

**Controller Family**:
A microcontroller platform that shares one firmware adapter and protocol
behavior, such as ESP32-S3 or RP2040. One Controller Family may support many
Board Profiles.
_Avoid_: Device type, board name

**Board Profile**:
Kivo's built-in definition of one concrete controller board, including its
Controller Family and board-specific identity and capabilities. Board Profiles
ship with Kivo rather than being installed by users.
_Avoid_: Controller Family, Controller Profile, Device Profile, plugin

**Hardware Profile**:
A controller-specific wiring topology and input binding contained in a **Device
Profile**. Every Hardware Profile targets one Board Profile, and one Device
Profile may contain multiple wiring variants for the same board.
_Avoid_: Board Profile, Device Profile, pin list

**Learning Session**:
A temporary scan bound to one explicit Device and one Hardware Profile. It
produces editor draft bindings and suspends only that Device; it never applies
results to other Devices until the draft is saved.
_Avoid_: Global learning, automatic assignment

## Flagged Ambiguities

**Active model**:
Previously meant both the Editor Profile and the only runtime configuration.
Use Editor Profile or Runtime Assignment explicitly instead.

**Model**:
Previously meant a user-assignable keypad configuration and was easily confused
with an MCU model or AI model. Use Device Profile.

**Controller Profile**:
Previously combined MCU-family behavior with concrete board identity and
capabilities. Use Controller Family or Board Profile explicitly.

## Example Dialogue

> **Developer:** Which Device Profile is open in the editor?
>
> **Domain expert:** The red phone is the Editor Profile. It is also assigned to
> the ESP32-S3 Device on the reception desk.
>
> **Developer:** What happens when I edit the RP2040 phone used in the meeting
> room?
>
> **Domain expert:** That RP2040 phone becomes the Editor Profile. Its Device
> keeps its own Runtime Assignment, while every other Device continues running
> its assigned Device Profile.
>
> **Developer:** How do we know it is the same Device after reconnecting it?
>
> **Domain expert:** Its Device ID uses its Board Profile and hardware
> serial, not whichever port it receives today.
>
> **Developer:** Do I need to pair a new ESP32-S3 before it appears?
>
> **Domain expert:** No. Successful recognition enrolls the Device
> automatically, but it remains inactive until it receives a Runtime Assignment.
>
> **Developer:** Can the red phone Device Profile run on both ESP32-S3 and
> RP2040 Devices?
>
> **Domain expert:** Yes. Its Runtime Assignment selects the Hardware Profile
> whose Board Profile and wiring match that Device.
