import { Fragment } from "react"

import { Audio } from "./Audio"
import { ControlPanel, DISPLAY_SIGNALS } from "./ControlPanel"
import { Controller } from "./Controller"
import { KeyMatrix } from "./KeyMatrix"
import { UsbAndPower } from "./UsbAndPower"

export { DISPLAY_SIGNALS }

export const WorkbenchOne = () => (
  <board
    width="180mm"
    height="105mm"
    layers={4}
    defaultTraceWidth="0.25mm"
    minTraceWidth="0.2mm"
    autorouter={{
      preset: "auto_local",
      traceClearance: "0.2mm",
    }}
    autorouterEffortLevel="1x"
  >
    {[
      [-87, -30],
      [-87, -10],
      [-87, 10],
      [87, -30],
      [87, -10],
      [87, 10],
    ].map(([pcbX, pcbY], index) => (
      <Fragment key={`GND_STITCH_${index + 1}`}>
        <via
          name={`GND_STITCH_${index + 1}`}
          pcbX={pcbX}
          pcbY={pcbY}
          fromLayer="top"
          toLayer="bottom"
          holeDiameter="0.3mm"
          outerDiameter="0.6mm"
          connectsTo="net.GND"
        />
      </Fragment>
    ))}
    {[
      [-84, -20],
      [-84, 20],
      [84, -20],
      [84, 20],
    ].map(([pcbX, pcbY], index) => (
      <Fragment key={`V3V3_FEED_${index + 1}`}>
        <via
          name={`V3V3_FEED_${index + 1}`}
          pcbX={pcbX}
          pcbY={pcbY}
          fromLayer="top"
          toLayer="bottom"
          holeDiameter="0.3mm"
          outerDiameter="0.6mm"
          connectsTo="net.V3V3"
        />
      </Fragment>
    ))}
    <copperpour
      name="GND_PLANE"
      layer="inner1"
      connectsTo="net.GND"
      clearance="0.2mm"
      boardEdgeMargin="0.2mm"
    />
    <copperpour
      name="V3V3_PLANE"
      layer="inner2"
      connectsTo="net.V3V3"
      clearance="0.2mm"
      boardEdgeMargin="0.2mm"
    />
    <UsbAndPower />
    <Controller />
    <Audio />
    <ControlPanel />
    <KeyMatrix />
    <schematictext
      text="USB + POWER"
      fontSize={0.8}
      anchor="left"
      schX={-25}
      schY={21}
    />
    <schematictext
      text="CM108 AUDIO"
      fontSize={0.8}
      anchor="left"
      schX={10}
      schY={21}
    />
    <schematictext
      text="ESP32-S3 + CONTROLS"
      fontSize={0.8}
      anchor="left"
      schX={-2}
      schY={11}
    />
    <schematictext
      text="3 x 6 KEY MATRIX"
      fontSize={0.8}
      anchor="left"
      schX={-16}
      schY={-1.5}
    />
    {[
      [-85, 47.5],
      [85, 47.5],
      [-85, -47.5],
      [85, -47.5],
    ].map(([pcbX, pcbY], index) => (
      <Fragment key={index}>
        <hole
          name={`H_M3_${index + 1}`}
          diameter="3.2mm"
          pcbX={pcbX}
          pcbY={pcbY}
        />
      </Fragment>
    ))}
    <silkscreentext
      text="WORKBENCH ONE P0"
      fontSize="2.2mm"
      pcbX={0}
      pcbY={-48}
    />
    <silkscreentext text="USB" fontSize="1.4mm" pcbX={-77} pcbY={39} />
    <silkscreentext text="CONTROL" fontSize="1.4mm" pcbX={28} pcbY={18} />
    <silkscreentext text="VOICE" fontSize="1.4mm" pcbX={67} pcbY={29} />
    <silkscreentext text="MACROS" fontSize="1.4mm" pcbX={0} pcbY={15} />
    <silkscreentext
      text="ESP32-S3 ANTENNA KEEPOUT"
      fontSize="1mm"
      pcbX={-12}
      pcbY={48}
    />
  </board>
)

export default WorkbenchOne
