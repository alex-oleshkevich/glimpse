import {
  Applet,
  Column,
  Hero,
  StatusItem,
  Tile,
  Label,
  type TreeNode,
} from "../src/index.js";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  constructor() {
    super();
  }

  protected initialState(): CounterState {
    return { count: 0 };
  }

  protected async status(state: CounterState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "counter",
        icon: "view-refresh-symbolic",
        label: String(state.count),
      }),
    ];
  }

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      children: [
        new Hero({
          icon: "view-refresh-symbolic",
          title: "Counter",
          subtitle: `Value: ${state.count}`,
        }),
        new Label(`Count = ${state.count}`),
        new Tile({
          primary: "Increment",
          left_icon: "list-add-symbolic",
          on_click: async () => {
            await this.setState({ count: this.state.count + 1 });
          },
        }),
      ],
    });
  }
}

void new CounterApplet().run();
