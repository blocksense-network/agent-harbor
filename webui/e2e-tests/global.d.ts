interface CSSStyleDeclaration {
  backgroundColor: string;
}

declare function getComputedStyle(elt: Element, pseudoElt?: string | null): CSSStyleDeclaration;
