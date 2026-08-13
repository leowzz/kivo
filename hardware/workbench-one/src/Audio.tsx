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
  <group
    name="G_AUDIO"
    pcbX={0}
    pcbY={0}
    pcbPositionAnchor="center"
    pack={false}
  >
    <chip
      name="U_AUDIO"
      manufacturerPartNumber="CM108B"
      footprint="qfn16"
      pinLabels={AUDIO_PIN_LABELS}
      pcbX={66}
      pcbY={40}
    />
    <trace from="U_AUDIO.VDD5" to="net.VBUS" />
    <trace from="U_AUDIO.VDD33" to="net.V3V3" />
    <trace from="U_AUDIO.GND" to="net.GND" />
    <trace from="U_AUDIO.USB_DP" to="net.USB_AUDIO_DP" />
    <trace from="U_AUDIO.USB_DM" to="net.USB_AUDIO_DM" />

    <crystal
      name="Y_AUDIO"
      frequency="12MHz"
      loadCapacitance="12pF"
      footprint="crystal4_px2.5mm_py2mm"
      pcbX={55}
      pcbY={40}
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
    />
    <resistor
      name="R_MIC_BIAS"
      resistance="2.2k"
      footprint="0603"
      pcbX={75}
      pcbY={36}
    />
    <capacitor
      name="C_RCV"
      capacitance="100uF"
      footprint="1206"
      pcbX={79}
      pcbY={36}
    />
    <resistor
      name="R_RCV"
      resistance="100"
      footprint="0603"
      pcbX={83}
      pcbY={36}
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
      pcbX={69}
      pcbY={33}
    />
    <trace from="C_AUDIO.pin1" to="net.V3V3" />
    <trace from="C_AUDIO.pin2" to="net.GND" />
  </group>
)
