import { Hero, Indicator, Popover, Row, run, Section } from "glimpse-sdk";

function App() {
  return (
    <>
      <Indicator icon={ "application-x-addon-symbolic" } tooltip={ "test" }>{ "test" }</Indicator>
      <Popover>
        <Hero icon={ "application-x-addon-symbolic" } title={ "test" } />
        <Section title={ "test" }>
          <Row title="It works" />
        </Section>
      </Popover>
    </>
  );
}

await run(<App />);
