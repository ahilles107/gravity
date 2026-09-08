import type { Story } from "@ladle/react";
import TableHead from "./TableHead";

export const Columns: Story = () => (
  <table className="runs-table">
    <TableHead columns={["Name", "State", "Created"]} />
    <tbody>
      <tr>
        <td>alice</td>
        <td>ready</td>
        <td>May 1, 09:00</td>
      </tr>
    </tbody>
  </table>
);

export const WithActionColumn: Story = () => (
  <table className="runs-table">
    <TableHead columns={["Name", "State"]} actionLabel="Actions" />
    <tbody>
      <tr>
        <td>alice</td>
        <td>ready</td>
        <td>
          <button type="button" className="btn btn-small">
            Retry
          </button>
        </td>
      </tr>
    </tbody>
  </table>
);
