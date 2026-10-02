import {
  Addon,
  Container,
  Indicator,
  Label,
  Popover,
  Switch,
  run,
  useState,
} from "glimpse-sdk";

function WidgetDemo() {
  const [wide, setWide] = useState(false);
  return (
    <>
      <Indicator icon="applications-graphics-symbolic" text="ext" />
      <Popover
        icon="applications-graphics-symbolic"
        title="Widget demo"
        subtitle="Popover shell slots"
        size={wide ? "wide" : "regular"}
      >
        <Addon>
          <Switch active={wide} onToggle={setWide} />
        </Addon>
        <Container height={48} background="red" color="light" align="center">
          <Label text="body 1" />
        </Container>
        <Container height={48} background="green" color="dark" align="center">
          <Label text="body 2" />
        </Container>
        <Container height={48} background="blue" color="light" align="center">
          <Label text="body 3" />
        </Container>
        <Container
          height={1024}
          background="yellow"
          color="dark"
          align="center"
        >
          <Label text="tall" />
        </Container>
      </Popover>
    </>
  );
}

run(<WidgetDemo />);
