import { fourPinHeaderFootprint } from "./footprints"

const AUDIO_PIN_LABELS = {
  pin1: "VDD5",
  pin2: "VDD33",
  pin3: "GND",
  pin4: "USB_DP",
  pin5: "USB_DM",
  pin6: "XTAL_IN",
  pin7: "XTAL_OUT",
  pin8: "MIC_IN",
  pin9: "MIC_BIAS",
  pin10: "HP_LEFT",
  pin11: "HP_RIGHT",
  pin12: "VREF",
  pin13: "LED",
  pin14: "GPIO1",
  pin15: "GPIO2",
  pin16: "NC",
} as const

export const Audio = () => (
  <>
    <chip
      name="U_AUDIO"
      manufacturerPartNumber="CM108B"
      footprint="qfn16"
      pinLabels={AUDIO_PIN_LABELS}
      pcbX={66}
      pcbY={40}
      schX={16.5}
      schY={18}
    />
    <trace from="U_AUDIO.VDD5" to="net.VBUS" thickness="0.5mm" />
    <trace from="U_AUDIO.VDD33" to="net.V3V3" thickness="0.5mm" />
    <resistor
      name="R_AUDIO_GND"
      resistance="0"
      footprint="0603"
      pcbX={62}
      pcbY={39.75}
      schX={13.5}
      schY={20}
    />
    <trace
      from="U_AUDIO.GND"
      to="R_AUDIO_GND.pin2"
      thickness="0.5mm"
      pcbStraightLine
    />
    <trace from="R_AUDIO_GND.pin1" to="net.GND" thickness="0.5mm" />

    <crystal
      name="Y_AUDIO"
      frequency="12MHz"
      loadCapacitance="12pF"
      footprint="crystal4_px2.5mm_py2mm"
      pcbX={66}
      pcbY={34.5}
      schX={12}
      schY={18}
    />
    <trace from="U_AUDIO.XTAL_IN" to="Y_AUDIO.pin1" />
    <trace from="U_AUDIO.XTAL_OUT" to="Y_AUDIO.pin2" />

    <chip
      name="J_HANDSET"
      footprint={fourPinHeaderFootprint}
      pinLabels={{
        pin1: "MIC_POS",
        pin2: "MIC_GND",
        pin3: "RCV_POS",
        pin4: "RCV_GND",
      }}
      pcbX={76}
      pcbY={45}
      schX={21.5}
      schY={18}
    />
    <resistor
      name="R_MIC_BIAS"
      resistance="2.2k"
      footprint="0603"
      pcbX={75}
      pcbY={36}
      schX={14}
      schY={13.5}
    />
    <capacitor
      name="C_RCV"
      capacitance="100uF"
      footprint="1206"
      pcbX={79}
      pcbY={36}
      schX={18}
      schY={13.5}
      schOrientation="vertical"
    />
    <resistor
      name="R_RCV"
      resistance="100"
      footprint="0603"
      pcbX={83}
      pcbY={36}
      schX={21}
      schY={13.5}
    />
    <trace from="U_AUDIO.MIC_BIAS" to="R_MIC_BIAS.pin1" />
    <trace from="R_MIC_BIAS.pin2" to="J_HANDSET.MIC_POS" />
    <trace from="U_AUDIO.MIC_IN" to="J_HANDSET.MIC_POS" />
    <trace from="J_HANDSET.MIC_GND" to="net.GND" />
    <trace from="U_AUDIO.HP_LEFT" to="C_RCV.pin1" />
    <trace from="C_RCV.pin2" to="R_RCV.pin1" />
    <trace from="R_RCV.pin2" to="J_HANDSET.RCV_POS" />
    <trace from="J_HANDSET.RCV_GND" to="net.GND" />

    <capacitor
      name="C_AUDIO"
      capacitance="10uF"
      footprint="0603"
      pcbX={70.5}
      pcbY={33}
      pcbRotation={180}
      schX={11}
      schY={13.5}
      schOrientation="vertical"
    />
    <trace from="C_AUDIO.pin1" to="net.V3V3" thickness="0.5mm" />
    <trace from="C_AUDIO.pin2" to="net.GND" thickness="0.5mm" />
  </>
)
