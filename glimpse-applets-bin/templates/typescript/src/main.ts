import {
  Applet,
  Button,
  Column,
  Hero,
  Icon,
  StatusItem,
  type TreeNode,
} from "glimpse-sdk";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  protected initialState(): CounterState {
    return { count: 0 };
  }

  constructor() {
    super();
    this.onClick("increment", async () => {
      await this.setState({ count: this.state.count + 1 });
    });
  }

  protected async status(state: CounterState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "counter",
        icon: Icon.name("view-refresh-symbolic"),
        label: String(state.count),
      }),
    ];
  }

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      spacing: 8,
      children: [
        new Hero({ title: "__NAME__", subtitle: `Value: ${state.count}` }),
        new Button({ id: "increment", label: "Increment" }),
      ],
    });
  }
}

await new CounterApplet().run();
