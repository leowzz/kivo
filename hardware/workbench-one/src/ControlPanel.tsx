import { Fragment } from "react"

import {
  displayModuleFootprint,
  toggleSwitchFootprint,
  twoPinHeaderFootprint,
} from "./footprints"

export const DISPLAY_SIGNALS = [
  "CON",
  "SDA",
  "SCL",
  "PSH",
  "TRA",
  "TRB",
  "BAK",
  "GND",
  "3V3",
] as const

const inputPullups = ["CON", "PSH", "TRA", "TRB", "BAK"] as const

export const ControlPanel = () => (
  <>
    <chip
      name="J_DISPLAY"
      footprint={displayModuleFootprint}
      pinLabels={Object.fromEntries(
        DISPLAY_SIGNALS.map((signal, index) => [`pin${index + 1}`, signal]),
      )}
      pcbX={35}
      pcbY={35}
      schX={12}
      schY={8}
    />

    {DISPLAY_SIGNALS.slice(0, 7).map((signal) => (
      <Fragment key={signal}>
        <trace from={`J_DISPLAY.${signal}`} to={`net.${signal}`} />
      </Fragment>
    ))}
    <trace from="J_DISPLAY.GND" to="net.GND" thickness="0.5mm" />
    <trace from="J_DISPLAY.3V3" to="net.V3V3" thickness="0.5mm" />

    {["SDA", "SCL", ...inputPullups].map((signal, index) => (
      <Fragment key={signal}>
        <resistor
          name={`R_${signal}`}
          resistance={signal === "SDA" || signal === "SCL" ? "4.7k" : "10k"}
          footprint="0603"
          pcbX={5 + index * 3.2}
          pcbY={signal === "SCL" ? 21 : 19}
          schX={6 + index * 2}
          schY={5}
          schOrientation="vertical"
        />
        <trace from={`R_${signal}.pin1`} to="net.V3V3" />
        <trace from={`R_${signal}.pin2`} to={`net.${signal}`} />
      </Fragment>
    ))}

    {[0, 1, 2].map((index) => (
      <Fragment key={index}>
        <chip
          name={`SW_MODE${index + 1}`}
          footprint={toggleSwitchFootprint}
          pinLabels={{ pin1: "ON", pin2: "COMMON", pin3: "ALT" }}
          pcbX={-68 + index * 19}
          pcbY={37}
          schX={8 + index * 5}
          schY={1}
        />
        <resistor
          name={`R_MODE${index + 1}`}
          resistance="10k"
          footprint="0603"
          pcbX={-68 + index * 19}
          pcbY={29}
          schX={8 + index * 5}
          schY={3}
          schOrientation="vertical"
        />
        <trace from={`SW_MODE${index + 1}.COMMON`} to="net.GND" />
        <trace from={`SW_MODE${index + 1}.ON`} to={`net.MODE${index + 1}`} />
        <trace from={`R_MODE${index + 1}.pin1`} to="net.V3V3" />
        <trace from={`R_MODE${index + 1}.pin2`} to={`net.MODE${index + 1}`} />
      </Fragment>
    ))}

    <chip
      name="J_HOOK"
      footprint={twoPinHeaderFootprint}
      pinLabels={{ pin1: "HOOK", pin2: "GND" }}
      pcbX={-78}
      pcbY={24}
      schX={23}
      schY={1}
    />
    <resistor
      name="R_HOOK"
      resistance="10k"
      footprint="0603"
      pcbX={-71}
      pcbY={24}
      schX={23}
      schY={3}
      schOrientation="vertical"
    />
    <trace from="J_HOOK.HOOK" to="net.HOOK" />
    <trace from="J_HOOK.GND" to="net.GND" />
    <trace from="R_HOOK.pin1" to="net.V3V3" />
    <trace from="R_HOOK.pin2" to="net.HOOK" />
  </>
)
