import { Circuit } from "tscircuit"
import { beforeAll, describe, expect, it } from "vitest"

import { DISPLAY_SIGNALS, WorkbenchOne } from "../src/WorkbenchOne"
import KeyMatrixEntrypoint from "../src/KeyMatrix"
import * as UsbAndPowerModule from "../src/UsbAndPower"

const renderCircuit = async () => {
  const circuit = new Circuit()
  circuit.add(<WorkbenchOne />)
  await circuit.renderUntilSettled()
  return circuit.getCircuitJson()
}

type CircuitJson = Awaited<ReturnType<typeof renderCircuit>>

let renderedCircuit: CircuitJson

const getRenderedCircuit = async () => renderedCircuit ?? renderCircuit()

const getSchematicComponents = (circuitJson: Awaited<ReturnType<typeof renderCircuit>>) => {
  const sourceNames = new Map(
    circuitJson
      .filter((element) => element.type === "source_component")
      .map((element) => [element.source_component_id, element.name]),
  )

  return circuitJson
    .filter((element) => element.type === "schematic_component")
    .map((element) => ({
      name: sourceNames.get(element.source_component_id ?? ""),
      center: element.center,
      size: element.size,
    }))
}

describe("Workbench One P0", () => {
  beforeAll(async () => {
    renderedCircuit = await renderCircuit()
  }, 180_000)

  it("exposes KeyMatrix as a browser-evaluable default component", () => {
    expect(typeof KeyMatrixEntrypoint).toBe("function")
  })

  it("renders the complete workstation structure", async () => {
    const circuitJson = await getRenderedCircuit()
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

  it("keeps schematic symbols separated and the keys in a 3 by 6 grid", async () => {
    const circuitJson = await getRenderedCircuit()
    const components = getSchematicComponents(circuitJson)
    const clearance = 0.15
    const overlaps: string[] = []

    for (let leftIndex = 0; leftIndex < components.length; leftIndex += 1) {
      const left = components[leftIndex]
      for (
        let rightIndex = leftIndex + 1;
        rightIndex < components.length;
        rightIndex += 1
      ) {
        const right = components[rightIndex]
        const overlapsX =
          Math.abs(left.center.x - right.center.x) <
          (left.size.width + right.size.width) / 2 + clearance
        const overlapsY =
          Math.abs(left.center.y - right.center.y) <
          (left.size.height + right.size.height) / 2 + clearance

        if (overlapsX && overlapsY) {
          overlaps.push(`${left.name}/${right.name}`)
        }
      }
    }

    expect(overlaps).toEqual([])

    const switches = components.filter(({ name }) => name?.startsWith("SW_K"))
    const keyRows = new Map<number, number>()
    for (const { center } of switches) {
      const row = Number(center.y.toFixed(2))
      keyRows.set(row, (keyRows.get(row) ?? 0) + 1)
    }

    expect([...keyRows.values()].sort()).toEqual([6, 6, 6])
  })

  it("does not expose code-organization groups in the PCB viewer", async () => {
    const circuitJson = await getRenderedCircuit()
    const visibleGroups = circuitJson
      .filter((element) => element.type === "pcb_group")
      .map((element) => element.name)

    expect(visibleGroups).toEqual([])
  })

  it("faces the USB-C receptacle outward and places it on the top edge", async () => {
    const circuitJson = await getRenderedCircuit()
    const usbSource = circuitJson
      .filter((element) => element.type === "source_component")
      .find((element) => element.name === "J_USB")
    const usbPcbComponent = circuitJson
      .filter((element) => element.type === "pcb_component")
      .find(
        (element) =>
          element.source_component_id === usbSource?.source_component_id,
      )

    expect(usbPcbComponent?.rotation).toBe(0)
    expect(usbPcbComponent?.cable_insertion_center?.y).toBeGreaterThan(
      usbPcbComponent?.center.y ?? Number.POSITIVE_INFINITY,
    )
    expect(
      (usbPcbComponent?.center.y ?? 0) + (usbPcbComponent?.height ?? 0) / 2,
    ).toBeCloseTo(51.9, 3)
  })

  it("routes PCB copper without routing errors and defines the USB links", async () => {
    const circuitJson = await getRenderedCircuit()
    const routingErrors = circuitJson.filter(
      (element) =>
        element.type.includes("autorouting_error") ||
        element.type.includes("trace_missing_error") ||
        element.type.includes("trace_error") ||
        element.type.includes("clearance_error"),
    )
    const usbPairs = (
      UsbAndPowerModule as typeof UsbAndPowerModule & {
        USB_DIFFERENTIAL_PAIRS?: readonly unknown[]
      }
    ).USB_DIFFERENTIAL_PAIRS
    const usbLinks = (
      UsbAndPowerModule as typeof UsbAndPowerModule & {
        USB_LINKS?: readonly unknown[]
      }
    ).USB_LINKS

    expect(circuitJson.filter((element) => element.type === "pcb_trace").length).toBeGreaterThan(0)
    expect(routingErrors).toEqual([])
    expect(usbLinks).toHaveLength(3)
    expect(usbPairs).toHaveLength(2)
  })

  it("uses four layers with dedicated inner GND and 3V3 planes", async () => {
    const circuitJson = await getRenderedCircuit()
    const board = circuitJson.find((element) => element.type === "pcb_board")
    const netNames = new Map(
      circuitJson.flatMap((element) =>
        element.type === "source_net" && element.name
          ? [[element.source_net_id, element.name] as const]
          : [],
      ),
    )
    const innerPlanes = circuitJson.flatMap((element) =>
      element.type === "pcb_copper_pour" && element.source_net_id
        ? [
            {
              layer: element.layer,
              connectsTo: netNames.get(element.source_net_id),
            },
          ]
        : [],
    )

    expect(board?.num_layers).toBe(4)
    expect(innerPlanes).toEqual(
      expect.arrayContaining([
        { layer: "inner1", connectsTo: "GND" },
        { layer: "inner2", connectsTo: "V3V3" },
      ]),
    )

    const planeVias = circuitJson.flatMap((element) =>
      element.type === "pcb_via" && element.source_net_id
        ? [
            {
              net: netNames.get(element.source_net_id),
              layers: element.layers,
            },
          ]
        : [],
    )
    expect(planeVias.filter(({ net }) => net === "GND").length).toBeGreaterThanOrEqual(6)
    expect(planeVias.filter(({ net }) => net === "V3V3").length).toBeGreaterThanOrEqual(4)
    expect(
      planeVias.every(({ layers }) =>
        (["top", "inner1", "inner2", "bottom"] as const).every((layer) =>
          layers.includes(layer),
        ),
      ),
    ).toBe(true)

    const usedLayers = new Set(
      circuitJson.flatMap((element) =>
        element.type === "pcb_trace"
          ? element.route.flatMap((point) =>
              point.route_type === "wire" ? [point.layer] : [],
            )
          : [],
      ),
    )
    expect(usedLayers).toEqual(
      new Set(["top", "inner1", "inner2", "bottom"]),
    )
  })
})
