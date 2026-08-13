import { Fragment, type ReactElement } from "react"

export const mechanicalSwitchFootprint: ReactElement = (
  <footprint>
    <hole name="CENTER" diameter="4mm" pcbX={0} pcbY={0} />
    <hole name="LOC_L" diameter="1.7mm" pcbX={-5.08} pcbY={0} />
    <hole name="LOC_R" diameter="1.7mm" pcbX={5.08} pcbY={0} />
    <platedhole
      name="P1"
      shape="circle"
      holeDiameter="1.5mm"
      outerDiameter="2.4mm"
      pcbX={-3.81}
      pcbY={-2.54}
      portHints={["pin1"]}
    />
    <platedhole
      name="P2"
      shape="circle"
      holeDiameter="1.5mm"
      outerDiameter="2.4mm"
      pcbX={2.54}
      pcbY={-5.08}
      portHints={["pin2"]}
    />
    <silkscreenrect width="14mm" height="14mm" strokeWidth="0.3mm" />
  </footprint>
)

export const toggleSwitchFootprint: ReactElement = (
  <footprint>
    {[-5, 0, 5].map((pcbX, index) => (
      <Fragment key={index}>
        <platedhole
          shape="circle"
          holeDiameter="1.3mm"
          outerDiameter="2.4mm"
          pcbX={pcbX}
          pcbY={0}
          portHints={[`pin${index + 1}`]}
        />
      </Fragment>
    ))}
    <silkscreenrect width="13mm" height="8mm" strokeWidth="0.3mm" />
  </footprint>
)

export const displayModuleFootprint: ReactElement = (
  <footprint>
    {Array.from({ length: 9 }, (_, index) => (
      <Fragment key={index}>
        <platedhole
          shape="circle"
          holeDiameter="1mm"
          outerDiameter="1.8mm"
          pcbX={(index - 4) * 2.54}
          pcbY={-11}
          portHints={[`pin${index + 1}`]}
        />
      </Fragment>
    ))}
    <silkscreenrect width="70mm" height="28mm" strokeWidth="0.35mm" />
    <silkscreenrect
      pcbX={-11}
      pcbY={1}
      width="40mm"
      height="18mm"
      strokeWidth="0.25mm"
    />
    <silkscreencircle pcbX={19} pcbY={2} radius="7mm" strokeWidth="0.3mm" />
    <silkscreenrect
      pcbX={11}
      pcbY={-8}
      width="8mm"
      height="4mm"
      strokeWidth="0.25mm"
    />
    <silkscreenrect
      pcbX={27}
      pcbY={-8}
      width="8mm"
      height="4mm"
      strokeWidth="0.25mm"
    />
  </footprint>
)

export const twoPinHeaderFootprint: ReactElement = (
  <footprint>
    <platedhole
      shape="circle"
      holeDiameter="1mm"
      outerDiameter="1.8mm"
      pcbX={-1.27}
      pcbY={0}
      portHints={["pin1"]}
    />
    <platedhole
      shape="circle"
      holeDiameter="1mm"
      outerDiameter="1.8mm"
      pcbX={1.27}
      pcbY={0}
      portHints={["pin2"]}
    />
    <silkscreenrect width="6mm" height="3mm" strokeWidth="0.25mm" />
  </footprint>
)

export const fourPinHeaderFootprint: ReactElement = (
  <footprint insertionDirection="from_above">
    {Array.from({ length: 4 }, (_, index) => (
      <Fragment key={index}>
        <platedhole
          shape="circle"
          holeDiameter="1mm"
          outerDiameter="1.8mm"
          pcbX={(index - 1.5) * 2.54}
          pcbY={0}
          portHints={[`pin${index + 1}`]}
        />
      </Fragment>
    ))}
    <silkscreenrect width="12mm" height="4mm" strokeWidth="0.25mm" />
  </footprint>
)

export const esp32ModuleFootprint: ReactElement = (
  <footprint>
    {Array.from({ length: 12 }, (_, index) => (
      <Fragment key={`left-${index}`}>
        <smtpad
          shape="rect"
          width="2mm"
          height="0.9mm"
          pcbX={-9}
          pcbY={(index - 5.5) * 1.27}
          portHints={[`pin${index + 1}`]}
        />
      </Fragment>
    ))}
    {Array.from({ length: 12 }, (_, index) => (
      <Fragment key={`right-${index}`}>
        <smtpad
          shape="rect"
          width="2mm"
          height="0.9mm"
          pcbX={9}
          pcbY={(5.5 - index) * 1.27}
          portHints={[`pin${index + 13}`]}
        />
      </Fragment>
    ))}
    <silkscreenrect width="18mm" height="25.5mm" strokeWidth="0.3mm" />
    <silkscreenrect
      pcbY={9.5}
      width="17mm"
      height="5mm"
      strokeWidth="0.25mm"
    />
  </footprint>
)

export const usbCConceptFootprint: ReactElement = (
  <footprint insertionDirection="from_front">
    {Array.from({ length: 12 }, (_, index) => (
      <Fragment key={`usb-signal-${index}`}>
        <smtpad
          shape="rect"
          width="0.45mm"
          height="1.8mm"
          pcbX={(index - 5.5) * 0.65}
          pcbY={-1}
          portHints={[`pin${index + 1}`]}
        />
      </Fragment>
    ))}
    {[
      [-5, 0.2],
      [5, 0.2],
      [-5, -3.2],
      [5, -3.2],
    ].map(([pcbX, pcbY], index) => (
      <Fragment key={`usb-shell-${index}`}>
        <platedhole
          shape="circle"
          holeDiameter="0.9mm"
          outerDiameter="1.8mm"
          pcbX={pcbX}
          pcbY={pcbY}
          portHints={[`pin${index + 13}`]}
        />
      </Fragment>
    ))}
    <silkscreenrect
      pcbY={-2.5}
      width="10mm"
      height="5mm"
      strokeWidth="0.3mm"
    />
  </footprint>
)
