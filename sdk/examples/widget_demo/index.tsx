import {
  Addon,
  ActionRow,
  Button,
  Container,
  Dots,
  End,
  ExpanderRow,
  Image,
  Indicator,
  Label,
  MeterRow,
  Popover,
  Section,
  SliderRow,
  Start,
  Switch,
  ToggleRow,
  run,
  useState,
} from "glimpse-sdk";

function WidgetDemo() {
  const [wide, setWide] = useState(false);
  const [wifi, setWifi] = useState(true);
  const [remember, setRemember] = useState(false);
  const [bluetoothOpen, setBluetoothOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [stormOpen, setStormOpen] = useState(false);
  const [volume, setVolume] = useState(0.5);
  const [muted, setMuted] = useState(false);
  const [quietVolume, setQuietVolume] = useState(0.2);
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
        <Section title="Rows">
          <MeterRow
            icon="folder-symbolic"
            title="Normal row"
            subtitle="With a subtitle"
            value="Ready"
            tooltip="Static tooltip"
          />
          <MeterRow title="Warning row" value="1 234,56" tone="warning" typography="numeric" />
          <MeterRow title="Error row" value="a1:b2:c3" tone="error" typography="monospace" />
          <MeterRow title="Inactive warning row" tone="warning" inactive subtitle="Device out of range" />
          <ActionRow icon="emblem-system-symbolic" title="Open settings" />
          <ActionRow title="Forget device" tone="destructive" />
        </Section>
        <Section title="Toggles">
          <ToggleRow title="Wi-Fi" active={wifi} onToggle={setWifi} />
          <ToggleRow title="Remember" control="check" active={remember} onToggle={setRemember} />
          <ToggleRow title="Airplane mode" active inactive subtitle="Locked by the hardware switch" />
        </Section>
        <Section title="Meters">
          <MeterRow title="CPU" level={0.3} value="30%" />
          <MeterRow title="Memory" level={0.8} tone="warning" value="80%" />
          <MeterRow title="Disk" level={0.95} tone="error" value="95%" />
        </Section>
        <Section title="Drawers">
          <ExpanderRow icon="bluetooth-symbolic" title="Bluetooth device" open={bluetoothOpen} onToggle={setBluetoothOpen}>
            <End><Image icon="object-select-symbolic" /></End>
            <ActionRow title="Connect" />
            <MeterRow title="Battery" level={0.6} value="60%" />
            <ActionRow title="Forget" tone="destructive" />
            <ExpanderRow title="Details" open={detailsOpen} onToggle={setDetailsOpen}>
              <MeterRow title="Address" value="00:11:22:33:44:55" typography="monospace" />
            </ExpanderRow>
          </ExpanderRow>
          <ExpanderRow title="More devices" open={moreOpen} onToggle={setMoreOpen}>
            <ActionRow title="Pair new device" />
          </ExpanderRow>
          <ActionRow title="Design review" value="14:00" typography="numeric">
            <Start><Dots colors={["accent"]} size={12} max={1} /></Start>
          </ActionRow>
        </Section>
        <Section title="Sliders">
          <SliderRow
            value={volume}
            inactive={muted}
            subtitle={muted ? "Muted" : undefined}
            onChange={setVolume}
          >
            <Start><Button
              icon={muted ? "audio-volume-muted-symbolic" : "audio-volume-medium-symbolic"}
              tooltip={muted ? "Unmute" : "Mute"}
              className="flat"
              onClick={() => setMuted((current) => !current)}
            /></Start>
          </SliderRow>
          <SliderRow value={quietVolume} inactive tone="warning" onChange={setQuietVolume} />
        </Section>
        <ExpanderRow icon="dialog-warning-symbolic" tone="warning" tint titleLines={3} title="Storm warning: strong wind gusts up to 90 km/h expected across the region" subtitle="Until 18:00" open={stormOpen} onToggle={setStormOpen}>
          <MeterRow title="Details" subtitle="Strong wind expected" />
        </ExpanderRow>
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
