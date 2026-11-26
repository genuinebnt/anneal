import { Fragment, type ReactNode } from "react";

/** The one breadcrumb: where you are, from the area down. Pass links for the steps that can be opened, text for the rest.
 *  Its look is in styles/design.css; `className` is only for placing it. */
export function Crumbs({ items, className = "" }: { items: ReactNode[]; className?: string }) {
    return (
        <nav className={`ly-crumb${className ? ` ${className}` : ""}`} aria-label="Breadcrumb">
            {items.map((c, i) => (
                <Fragment key={i}>
                    {i > 0 && <i aria-hidden="true">/</i>}
                    <span>{c}</span>
                </Fragment>
            ))}
        </nav>
    );
}
