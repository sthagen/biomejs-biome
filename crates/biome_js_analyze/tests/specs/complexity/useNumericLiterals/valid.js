/* should not generate diagnostics */
parseInt(1);
parseInt(1, 3);
Number.parseInt(1);
Number.parseInt(1, 3);
0b111110111 === 503;
0o767 === 503;
0x1F7 === 503;
a[parseInt](1,2);
parseInt(foo);
parseInt(foo, 2);
Number.parseInt(foo);
Number.parseInt(foo, 2);
parseInt(11, 2);
Number.parseInt(1, 8);
parseInt(1e5, 16);
parseInt('11', '2');
Number.parseInt('11', '8');
parseInt(/foo/, 2);
parseInt(`11${foo}`, 2);
parseInt('11', 2n);
Number.parseInt('11', 8n);
parseInt('11', 16n);
parseInt(`11`, 16n);
parseInt(1n, 2);
class C { #parseInt; foo() { Number.#parseInt("111110111", 2); } }
