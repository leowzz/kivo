import { Fragment } from "react"

import { mechanicalSwitchFootprint } from "./footprints"

const KEY_PITCH = 19.05

export const KEY_LAYOUT = Array.from({ length: 18 }, (_, index) => {
  const row = Math.floor(index / 6)
  const column = index % 6
  return {
    id: String(index + 1).padStart(2, "0"),
    row,
    column,
    pcbX: (column - 2.5) * KEY_PITCH,
    pcbY: 6 - row * KEY_PITCH,
    schX: (column - 2.5) * 6,
    schY: -4 - row * 3,
  }
})

export const KeyMatrix = () => (
  <>
    {KEY_LAYOUT.map(({ id, row, column, pcbX, pcbY, schX, schY }) => (
      <Fragment key={id}>
        <chip
          name={`SW_K${id}`}
          footprint={mechanicalSwitchFootprint}
          pinLabels={{ pin1: "COL", pin2: "DIODE" }}
          pcbX={pcbX}
          pcbY={pcbY}
          schX={schX}
          schY={schY}
        />
        <diode
          name={`D_K${id}`}
          footprint="sod123"
          pcbX={pcbX + 6.5}
          pcbY={pcbY - 6.4}
          pcbRotation={90}
          schX={schX + 2.3}
          schY={schY}
        />
        <trace from={`SW_K${id}.COL`} to={`net.COL${column}`} />
        <trace from={`SW_K${id}.DIODE`} to={`D_K${id}.pin1`} />
        <trace from={`D_K${id}.pin2`} to={`net.ROW${row}`} />
      </Fragment>
    ))}
  </>
)

export default KeyMatrix
