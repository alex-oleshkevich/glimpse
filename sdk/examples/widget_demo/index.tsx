import { Indicator, Popover, run } from "glimpse-sdk";

function WidgetDemo() {
  return (
    <>
      <Indicator icon="applications-graphics-symbolic">ext</Indicator>
      <Popover icon="applications-graphics-symbolic" title="Widget demo" subtitle="Popover shell slots" />
    </>
  );
}

await run(<WidgetDemo />);
