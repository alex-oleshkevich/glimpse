import {
  Applet,
  Box,
  Button,
  Hero,
  Icon,
  Label,
  StatusItem,
  type TreeNode,
} from "../src/index.js";

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
    return Box.vertical(
      [
        new Hero({
          icon: Icon.name("view-refresh-symbolic"),
          title: "Counter",
          subtitle: `Value: ${state.count}`,
        }),
        new Label(`Count = ${state.count}`),
        new Button({
          id: "increment",
          label: "Increment",
          icon: "list-add-symbolic",
          variant: "primary",
        }),
      ],
      8,
    );
  }
}

void new CounterApplet().run();
