/* should not generate diagnostics */
export { Foo } from 'foo';

export type { Type1 } from './consistent-type-exports';

export { value1 } from './consistent-type-exports';

export type { value1 } from './consistent-type-exports';

const variable = 1;
class Class {}
enum Enum {}
function func() {}
namespace ns {
  export const x = 1;
}
export { variable, Class, Enum, func, ns };

type Alias = 1;
interface IFace {}
export type { Alias, IFace };

const foo = 1;
export type { foo };

namespace NonTypeNS {
  export const x = 1;
}
export { NonTypeNS };

function f2() {}
class Class2 {}
namespace typeNs {
    export type x = 1;
}
export type { f2, Class2, typeNs };

// we ignore ambient types that are values in non-ambient contexts
declare class AmbientClass {}
declare enum AmbientEnum {}
declare class AmbientFunction {}
export { AmbientClass, AmbientEnum, AmbientFunction }

export {}

function f3() {}
class Class3 {}
export { type Class3, f3 }

function f4() {}
export { f4 }

export { type T1, V1 } from "./mod.ts";
