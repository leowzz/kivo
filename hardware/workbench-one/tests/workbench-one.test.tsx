import { Circuit } from "tscircuit"
import { describe, expect, it } from "vitest"

import { DISPLAY_SIGNALS, WorkbenchOne } from "../src/WorkbenchOne"
import KeyMatrixEntrypoint from "../src/KeyMatrix"

const renderCircuit = async () => {
  const circuit = new Circuit()
  circuit.add(<WorkbenchOne />)
  await circuit.renderUntilSettled()
  return circuit.getCircuitJson()
}

describe("Workbench One P0", () => {
  it("exposes KeyMatrix as a browser-evaluable default component", () => {
    expect(typeof KeyMatrixEntrypoint).toBe("function")
  })

  it("renders the complete workstation structure", async () => {
    const circuitJson = await renderCircuit()
    const sourceComponents = circuitJson.filter(
      (element) => element.type === "source_component",
    )

    const named = (prefix: string) =>
      sourceComponents.filter(
        (element) =>
          "name" in element &&
          typeof element.name === "string" &&
          element.name.startsWith(prefix),
      )

    expect(named("SW_K")).toHaveLength(18)
    expect(named("D_K")).toHaveLength(18)
    expect(named("SW_MODE")).toHaveLength(3)
    expect(DISPLAY_SIGNALS).toEqual([
      "CON",
      "SDA",
      "SCL",
      "PSH",
      "TRA",
      "TRB",
      "BAK",
      "GND",
      "3V3",
    ])

    for (const componentName of [
      "J_USB",
      "U_HUB",
      "U_MCU",
      "U_AUDIO",
      "J_DISPLAY",
      "J_HANDSET",
      "J_HOOK",
    ]) {
      expect(named(componentName), componentName).toHaveLength(1)
    }

    expect(
      circuitJson.filter(
        (element) =>
          element.type === "pcb_hole" &&
          "hole_diameter" in element &&
          element.hole_diameter === 3.2,
      ),
    ).toHaveLength(4)

    expect(
      circuitJson.filter((element) => element.type.endsWith("_error")),
    ).toEqual([])
  })
})
