import { Fragment } from "react"

import { usbCConceptFootprint } from "./footprints"

const HUB_PIN_LABELS = {
  pin1: "VDD33",
  pin2: "GND",
  pin3: "UP_DP",
  pin4: "UP_DM",
  pin5: "P1_DP",
  pin6: "P1_DM",
  pin7: "P2_DP",
  pin8: "P2_DM",
  pin9: "XTAL_IN",
  pin10: "XTAL_OUT",
  pin11: "RESET_N",
  pin12: "VBUS_DET",
  pin13: "CFG1",
  pin14: "CFG2",
  pin15: "VDD18",
  pin16: "RBIAS",
} as const

export const USB_LINKS = ["USB_UPSTREAM", "USB_MCU", "USB_AUDIO"] as const

export const USB_DIFFERENTIAL_PAIRS = [
  {
    name: "USB_MCU",
    positiveConnection: "USB_MCU_DP",
    negativeConnection: "USB_MCU_DM",
  },
  {
    name: "USB_AUDIO",
    positiveConnection: "USB_AUDIO_DP",
    negativeConnection: "USB_AUDIO_DM",
  },
] as const

export const UsbAndPower = () => (
  <>
    <connector
      name="J_USB"
      standard="usb_c"
      manufacturerPartNumber="P0_USB_C_CONCEPT"
      footprint={usbCConceptFootprint}
      pinLabels={{
        pin1: "GND1",
        pin2: "VBUS1",
        pin3: "CC1",
        pin4: "DP1",
        pin5: "DM1",
        pin6: "SBU1",
        pin7: "SBU2",
        pin8: "DM2",
        pin9: "DP2",
        pin10: "CC2",
        pin11: "VBUS2",
        pin12: "GND2",
        pin13: "SHELL1",
        pin14: "SHELL2",
        pin15: "SHELL3",
        pin16: "SHELL4",
      }}
      pcbX={-70}
      pcbTopEdgeY={51.9}
      pcbRotation={0}
      schX={-24}
      schY={18}
    />

    <fuse
      name="F1"
      footprint="1206"
      currentRating="500mA"
      voltageRating="6V"
      pcbX={-66}
      pcbY={32}
      schX={-24}
      schY={13.5}
    />
    <trace from="J_USB.VBUS1" to="net.VBUS_RAW" thickness="0.5mm" />
    <trace from="J_USB.VBUS2" to="net.VBUS_RAW" thickness="0.5mm" />
    <trace from="F1.pin1" to="net.VBUS_RAW" thickness="0.5mm" />
    <trace from="F1.pin2" to="net.VBUS" thickness="0.5mm" />

    {(["CC1", "CC2"] as const).map((signal, index) => (
      <Fragment key={signal}>
        <resistor
          name={`R_${signal}`}
          resistance="5.1k"
          footprint="0603"
          pcbX={-74 + index * 4}
          pcbY={42}
          schX={-21 + index * 2}
          schY={13.5}
        />
        <trace from={`J_USB.${signal}`} to={`R_${signal}.pin1`} />
        <trace from={`R_${signal}.pin2`} to="net.GND" />
      </Fragment>
    ))}

    {(["GND1", "GND2", "SHELL1", "SHELL2", "SHELL3", "SHELL4"] as const).map(
      (pin) => (
        <Fragment key={pin}>
          <trace from={`J_USB.${pin}`} to="net.GND" />
        </Fragment>
      ),
    )}
    <trace from="J_USB.DP1" to="net.USB_UP_DP" />
    <trace from="J_USB.DP2" to="net.USB_UP_DP" />
    <trace from="J_USB.DM1" to="net.USB_UP_DM" />
    <trace from="J_USB.DM2" to="net.USB_UP_DM" />

    <chip
      name="U_ESD"
      footprint="sot563"
      pinLabels={{
        pin1: "DP",
        pin2: "DM",
        pin3: "GND",
        pin4: "VBUS",
        pin5: "NC1",
        pin6: "NC2",
      }}
      pcbX={-59}
      pcbY={45}
      schX={-20.5}
      schY={18}
    />
    <trace from="U_ESD.DP" to="net.USB_UP_DP" />
    <trace from="U_ESD.DM" to="net.USB_UP_DM" />
    <trace from="U_ESD.VBUS" to="net.VBUS" thickness="0.5mm" />
    <trace from="U_ESD.GND" to="net.GND" thickness="0.5mm" />

    <chip
      name="U_HUB"
      manufacturerPartNumber="USB2512B"
      footprint="qfn16"
      pinLabels={HUB_PIN_LABELS}
      pcbX={-45}
      pcbY={43}
      schX={-15}
      schY={18}
    />
    <trace from="U_HUB.VDD33" to="net.V3V3" thickness="0.5mm" />
    <trace from="U_HUB.GND" to="net.GND" thickness="0.5mm" />
    <trace from="U_HUB.UP_DP" to="net.USB_UP_DP" />
    <trace from="U_HUB.UP_DM" to="net.USB_UP_DM" />
    <trace
      name="USB_MCU_DP"
      from="U_HUB.P1_DP"
      to="U_MCU.USB_DP"
      schDisplayLabel="USB_MCU_DP"
    />
    <trace
      name="USB_MCU_DM"
      from="U_HUB.P1_DM"
      to="U_MCU.USB_DM"
      schDisplayLabel="USB_MCU_DM"
    />
    <trace
      name="USB_AUDIO_DP"
      from="U_HUB.P2_DP"
      to="U_AUDIO.USB_DP"
      schDisplayLabel="USB_AUDIO_DP"
    />
    <trace
      name="USB_AUDIO_DM"
      from="U_HUB.P2_DM"
      to="U_AUDIO.USB_DM"
      schDisplayLabel="USB_AUDIO_DM"
    />
    <trace from="U_HUB.VBUS_DET" to="net.VBUS" thickness="0.5mm" />
    {USB_DIFFERENTIAL_PAIRS.map((pair) => (
      <Fragment key={pair.name}>
        <differentialpair
          {...pair}
          maxLengthSkew="0.5mm"
          pcbTraceGap="0.2mm"
          maxUncoupledLength="3mm"
        />
      </Fragment>
    ))}

    <crystal
      name="Y_HUB"
      frequency="24MHz"
      loadCapacitance="12pF"
      footprint="crystal4_px2.5mm_py2mm"
      pcbX={-37}
      pcbY={43}
      schX={-10.5}
      schY={18}
    />
    <trace from="U_HUB.XTAL_IN" to="Y_HUB.pin1" />
    <trace from="U_HUB.XTAL_OUT" to="Y_HUB.pin2" />

    <chip
      name="U_REG"
      manufacturerPartNumber="AP2112K-3.3"
      footprint="sot23"
      pinLabels={{ pin1: "GND", pin2: "OUT", pin3: "IN" }}
      pcbX={-55}
      pcbY={27}
      schX={-16}
      schY={13.5}
    />
    <trace from="U_REG.IN" to="net.VBUS" thickness="0.5mm" />
    <trace from="U_REG.OUT" to="net.V3V3" thickness="0.5mm" />
    <trace from="U_REG.GND" to="net.GND" thickness="0.5mm" />

    {[
      { name: "C_VBUS", value: "10uF", x: -62, rail: "VBUS" },
      { name: "C_3V3", value: "10uF", x: -58, rail: "V3V3" },
      { name: "C_HUB", value: "100nF", x: -44, rail: "V3V3" },
    ].map(({ name, value, x, rail }) => (
      <Fragment key={name}>
        <capacitor
          name={name}
          capacitance={value}
          footprint="0603"
          pcbX={x}
          pcbY={31}
          schX={-13 + ["C_VBUS", "C_3V3", "C_HUB"].indexOf(name) * 2}
          schY={13.5}
          schOrientation="vertical"
        />
        <trace from={`${name}.pin1`} to={`net.${rail}`} thickness="0.5mm" />
        <trace from={`${name}.pin2`} to="net.GND" thickness="0.5mm" />
      </Fragment>
    ))}
  </>
)
