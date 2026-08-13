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

export const UsbAndPower = () => (
  <group
    name="G_USB_POWER"
    pcbX={0}
    pcbY={0}
    pcbPositionAnchor="center"
    pack={false}
  >
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
      pcbY={48}
      pcbRotation={0}
    />

    <fuse
      name="F1"
      footprint="1206"
      currentRating="500mA"
      voltageRating="6V"
      pcbX={-66}
      pcbY={32}
    />
    <trace from="J_USB.VBUS1" to="net.VBUS_RAW" />
    <trace from="J_USB.VBUS2" to="net.VBUS_RAW" />
    <trace from="F1.pin1" to="net.VBUS_RAW" />
    <trace from="F1.pin2" to="net.VBUS" />

    {(["CC1", "CC2"] as const).map((signal, index) => (
      <group
        key={signal}
        name={`G_${signal}`}
        pcbX={0}
        pcbY={0}
        pcbPositionAnchor="center"
        pack={false}
      >
        <resistor
          name={`R_${signal}`}
          resistance="5.1k"
          footprint="0603"
          pcbX={-74 + index * 4}
          pcbY={42}
        />
        <trace from={`J_USB.${signal}`} to={`R_${signal}.pin1`} />
        <trace from={`R_${signal}.pin2`} to="net.GND" />
      </group>
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
    />
    <trace from="U_ESD.DP" to="net.USB_UP_DP" />
    <trace from="U_ESD.DM" to="net.USB_UP_DM" />
    <trace from="U_ESD.VBUS" to="net.VBUS" />
    <trace from="U_ESD.GND" to="net.GND" />

    <chip
      name="U_HUB"
      manufacturerPartNumber="USB2512B"
      footprint="qfn16"
      pinLabels={HUB_PIN_LABELS}
      pcbX={-45}
      pcbY={43}
    />
    <trace from="U_HUB.VDD33" to="net.V3V3" />
    <trace from="U_HUB.GND" to="net.GND" />
    <trace from="U_HUB.UP_DP" to="net.USB_UP_DP" />
    <trace from="U_HUB.UP_DM" to="net.USB_UP_DM" />
    <trace from="U_HUB.P1_DP" to="net.USB_MCU_DP" />
    <trace from="U_HUB.P1_DM" to="net.USB_MCU_DM" />
    <trace from="U_HUB.P2_DP" to="net.USB_AUDIO_DP" />
    <trace from="U_HUB.P2_DM" to="net.USB_AUDIO_DM" />
    <trace from="U_HUB.VBUS_DET" to="net.VBUS" />

    <crystal
      name="Y_HUB"
      frequency="24MHz"
      loadCapacitance="12pF"
      footprint="crystal4_px2.5mm_py2mm"
      pcbX={-34}
      pcbY={43}
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
    />
    <trace from="U_REG.IN" to="net.VBUS" />
    <trace from="U_REG.OUT" to="net.V3V3" />
    <trace from="U_REG.GND" to="net.GND" />

    {[
      { name: "C_VBUS", value: "10uF", x: -62, rail: "VBUS" },
      { name: "C_3V3", value: "10uF", x: -58, rail: "V3V3" },
      { name: "C_HUB", value: "100nF", x: -44, rail: "V3V3" },
    ].map(({ name, value, x, rail }) => (
      <group
        key={name}
        name={`G_${name}`}
        pcbX={0}
        pcbY={0}
        pcbPositionAnchor="center"
        pack={false}
      >
        <capacitor
          name={name}
          capacitance={value}
          footprint="0603"
          pcbX={x}
          pcbY={31}
        />
        <trace from={`${name}.pin1`} to={`net.${rail}`} />
        <trace from={`${name}.pin2`} to="net.GND" />
      </group>
    ))}
  </group>
)
