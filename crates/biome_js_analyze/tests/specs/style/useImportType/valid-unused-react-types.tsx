/* should not generate diagnostics */
import * as React from "react";

function Component() {
    const onClick = (event: React.MouseEvent) => { };

    return <div onClick={onClick}></div>;
}
