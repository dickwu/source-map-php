<?php

namespace App\Controller;

use Hyperf\Di\Annotation\Inject;
use Hyperf\HttpServer\Annotation\PostMapping;

class ConsentController
{
    #[Inject]
    protected \App\Service\ConsentService $service;

    #[PostMapping(path: '/consents')]
    public function store(): array
    {
        return ['ok' => true];
    }
}
