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
    layers={2}
    routingDisabled
    schAutoLayoutEnabled
  >
    <UsbAndPower />
    <Controller />
    <Audio />
    <ControlPanel />
    <KeyMatrix />
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
