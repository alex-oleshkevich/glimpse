import { Hero, Indicator, Popover, Row, run, Section } from "@glimpse/applet";

function App() {
  return (
    <>
      <Indicator icon={{icon}} tooltip={{name}}>{ {{name}} }</Indicator>
      <Popover>
        <Hero icon={{icon}} title={{name}} />
        <Section title={{name}}>
          <Row title="It works" />
        </Section>
      </Popover>
    </>
  );
}

await run(<App />);
