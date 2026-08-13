import { Fragment } from "react"

import { esp32ModuleFootprint } from "./footprints"

const MCU_PIN_LABELS = {
  pin1: "V3V3",
  pin2: "GND",
  pin3: "USB_DP",
  pin4: "USB_DM",
  pin5: "ROW0",
  pin6: "ROW1",
  pin7: "ROW2",
  pin8: "COL0",
  pin9: "COL1",
  pin10: "COL2",
  pin11: "COL3",
  pin12: "COL4",
  pin13: "COL5",
  pin14: "SDA",
  pin15: "SCL",
  pin16: "CON",
  pin17: "PSH",
  pin18: "TRA",
  pin19: "TRB",
  pin20: "BAK",
  pin21: "MODE1",
  pin22: "MODE2",
  pin23: "MODE3",
  pin24: "HOOK",
} as const

const MCU_SIGNALS = [
  "ROW0",
  "ROW1",
  "ROW2",
  "COL0",
  "COL1",
  "COL2",
  "COL3",
  "COL4",
  "COL5",
  "SDA",
  "SCL",
  "CON",
  "PSH",
  "TRA",
  "TRB",
  "BAK",
  "MODE1",
  "MODE2",
  "MODE3",
  "HOOK",
] as const

export const Controller = () => (
  <>
    <chip
      name="U_MCU"
      manufacturerPartNumber="ESP32-S3-WROOM-1"
      footprint={esp32ModuleFootprint}
      pinLabels={MCU_PIN_LABELS}
      pcbX={-12}
      pcbY={38}
      schX={0}
      schY={8}
    />
    <trace from="U_MCU.V3V3" to="net.V3V3" thickness="0.5mm" />
    <trace from="U_MCU.GND" to="net.GND" thickness="0.5mm" />
    {MCU_SIGNALS.map((signal) => (
      <Fragment key={signal}>
        <trace from={`U_MCU.${signal}`} to={`net.${signal}`} />
      </Fragment>
    ))}
    <capacitor
      name="C_MCU"
      capacitance="10uF"
      footprint="0603"
      pcbX={0}
      pcbY={31}
      schX={0}
      schY={4.5}
      schOrientation="vertical"
    />
    <trace from="C_MCU.pin1" to="net.V3V3" thickness="0.5mm" />
    <trace from="C_MCU.pin2" to="net.GND" thickness="0.5mm" />
    <keepout shape="rect" pcbX={-12} pcbY={48} width="20mm" height="5mm" />
  </>
)
