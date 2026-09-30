import { Hero, Indicator, Popover, Row, run, Section } from "glimpse-sdk";

function App() {
  return (
    <>
      <Indicator icon={ {{ icon | tojson }} } tooltip={ {{ name | tojson }} }>{ {{ name | tojson }} }</Indicator>
      <Popover>
        <Hero icon={ {{ icon | tojson }} } title={ {{ name | tojson }} } />
        <Section title={ {{ name | tojson }} }>
          <Row title="It works" />
        </Section>
      </Popover>
    </>
  );
}

await run(<App />);
